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


## 🚦 CURRENT DIRECTION — Tom, 7 Aug: LANGUAGES MUST BE EXTENSIBLE

*"Can we make sure that our language support is extensible? I don't want to be
hyper-specialising and hard coding in keywords. We want a Zed-like extensibility
system. We've got our own language that we work with, AWL, and we've got
tree-sitter packages for that."*

This **supersedes the assumption behind #61 and #62** — that adding a language
means editing an enum. It does not invalidate what landed; it changes where the
design is going.

**The map is `docs/IN-FLIGHT-languages.md`.** Ground verified, three tiers
priced, decisions L-0 to L-8 numbered for Tom. Read it before touching anything
language-shaped. Headlines:

- The Rust side is fully static, and `COMPILED[language.index()][kind.index()]`
  is a **fixed-size array typed from `Language::COUNT`** — that is the
  structural blocker, not the enum itself.
- **The web face already loads wasm grammars at runtime.** The static half is
  the *native* half, which is the reverse of what `embedded.rs`'s doc assumes.
- **chiron is fully static too** — Tom named it as the source, and it is the
  same closed enum, only wider. Its reusable idea is `build.rs` compiling
  vendored `parser.c`, which is the answer to AWL having no published crate.
- 21 Zed `config.toml` manifests are already vendored and **nothing reads them**,
  though they carry three of the four hard-coded tables as data.

**Do not start removing the enum before Tom rules on L-0** (which tier).

---

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
| `225cd1a` | **the document is fenced out of the band a face reserved** |
| `2ad81ab` | **`iridium-explorer` — the filesystem source, off the frame thread** |
| `d914bd7` | **the file explorer panel, on `Ctrl+Alt+E`** |

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

---

## 🧱 #51 — THE RESERVED BAND IS NOW FENCED, not just offset — `225cd1a`

**`TextBounds` clips text. Every quad is geometry, and it clipped none of
them.** The gutter background, its change bars, the diff line backgrounds,
the selection highlights and the caret each tested visibility against `0.0`
— the window's top edge, which equals the band's bottom edge for exactly as
long as no face reserves anything.

**The case they diverge on is a *scroll*.** A row's Y is
`top_inset + row - scroll`, so a row that started below the band moves into
it. The builders' cull asks "is this row in the window?"; nobody asked "is it
in the band?". The left band has no such case — nothing scrolls sideways —
which is why `left_inset.rs` has asserted an empty band since #50 and the Y
axis still leaked.

**The fix is one scissor rect on the compositor's own pass**, not five
clamped quads: five copies of one comparison is how this bug was born. Set
*after* the clear, so a reserved band still returns page-coloured — the
document is fenced out, not the frame. The overlay's pass is separate and
untouched, which is what lets a face draw in the band it claimed.

`reserved_edge` **truncates**, matching `pixel_to_bound`, which is what the
text areas clip with. A band 44.5 pixels deep must clip a glyph and the caret
beside it at the same row.

Two tests in `top_inset.rs`, both proven red first. The second exists because
the first fails at (0, 0) on the gutter's background quad and would keep
reporting a leak while every content quad was already correct — turn the
gutter off and the content answers for itself.

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

### Build order — step 1 is DONE, `2ad81ab`

Filesystem `TreeSource` ✅ → the popover showing it ✅ (`d914bd7`) → fuzzy
filtering ✅ (`1148e73`, `3c269fa`, `b33e89e`) → regex ✅ (7 Aug) → **the
editable-buffer half is what is left**, and it is last because it is the part
that touches the disk.

#### Step 2 — what landed, and the defect it turned up

`explorer.togglePanel` is a kernel host command beside `palette.open`, bound
to **`Ctrl+Alt+E`** (the undo tree's chord shape — both are toggled panels).
`DEFAULT_KEYMAP_BINDING_COUNT` went 65 → 66. The panel is
`apps/iridium-desktop/src/file_tree/` (mod 32 · panel 423 · tests 232) and
composes into the same `PanelContent` the palette and undo tree use.

**The defect, caught by a test written first.** `iridium-tree`'s fourth
`TreeSource` rule: a node that promised children and produced none is treated
as a leaf *from then on*. Expanding a directory whose read is still in flight
is exactly that — it never opens again. The first version did it to the root
in `open` and to every directory on `Enter`. Now every expansion goes through
`FileExplorer::open_row`, gated on the new `FileTree::is_listed`: listed
expands now, unlisted posts the read and is remembered in `wanted`, and the
poll that brings the listing opens it. **Anyone adding an expansion path must
use `open_row`, never `Tree::expand` directly.**

The panel is the only one that polls: `redraw` drains before composing panels
and asks for another frame while — and only while — `is_waiting` is true.
Closing *drops* it (it owns a reader thread and an arena), unlike the other
two which are hidden.

**RULED 7 Aug 2026 — `Enter` on a directory toggles.** Tom: *"Can you enter
on a folder? I thought we just had unfold."* Descend-and-re-root is not a
thing this panel does. The shipped behaviour was already the ruling, so
nothing changed but the comment on `activate` and a test that now pins the
*close* half of the toggle — the half that tells a toggle from a descend.

**RULED 7 Aug 2026 — the explorer takes the open-a-file chord.** Tom: *"I
would be happy for it to replace command O, or have it be option O or
something like that."*

**And ⌘O does not exist.** Checked, not assumed: `open_file` has exactly two
callers — a drop (`files.rs::dropped`) and this panel (`keyboard.rs:151`).
There is no file-open dialog and no `O` binding anywhere in the repo. A
comment on `drive_explorer` claimed `Ctrl+O` was a third caller; it was
wrong and has been corrected. So "replace ⌘O" costs nothing — there is
nothing to displace, and the explorer is already the only keyboard route to
opening a file.

#### What `iridium-explorer` gives you

A new crate, `crates/iridium-explorer` — five files, largest 265. Its own
crate because both neighbours are *documented* not to know this: `iridium-tree`
has no dependencies so a tree cannot learn whether it shows files or syntax
nodes, and `iridium-file` has none so the file layer cannot learn what a face
does with the bytes. Native-only besides — a browser has no directory.

**`FileTree` implements `iridium_tree::TreeSource` and never blocks.**
`iridium-tree`'s own docs state the rule and say they cannot enforce it. A
listing that is not already known is posted to a reader thread; the face's
loop is three lines:

```rust
if files.drain() { tree.refresh(&mut files); }
```

`drain` returns `true` exactly when `Tree::refresh` is owed a call. The price
is a cold expand showing an empty directory for one frame — `NodeInfo::is_loading`
tells that apart from a directory that is genuinely empty, and a failed
listing carries the OS's own message rather than looking empty too.

**`NodeId` is an arena index and is never reused.** This is the thing the
editable half depends on: rows diff *by id*, and matching by name makes a
rename read as delete-plus-create — destroying a large file's contents and
writing them back rather than moving it. `reload` re-reads a directory while
every file that did not move keeps its id, and a test pins that.

Symlinks are `EntryKind::Symlink`, never `Directory`, so they are leaves and
never followed — which is how the acyclicity rule `iridium-tree` cannot check
is kept. A test builds a link to its own parent.

Ordering policy lives on the worker thread: directories first, then name
lowercased, then name exactly — the third key is what makes the order *total*,
and a non-total order would let two names differing only in case swap between
reads and break the stability the tree splices on.


### Deliberately not decided

- ~~Whether Enter on a directory descends in the popover or opens an Oil
  buffer in a tab.~~ **RULED 7 Aug 2026: it toggles.** A folder unfolds and
  refolds in place. Neither descending nor opening an Oil buffer in a tab —
  so the editable half, when it is built, edits the *popover's* rows rather
  than materialising a tab.
- ~~Whether this replaces `⌘O` or sits beside it.~~ **RULED 7 Aug 2026:** the
  explorer takes the open-a-file chord. ⌘O turned out never to have been
  bound, so there is nothing to replace.

## 🔎 POPOVER STEP 3 — FILTERING, landed 6 Aug, `1148e73`

**Tom's ruling stands: hierarchy, not a flat list.** He confirmed it over
Meridian on 6 Aug — *"the hierarchy call from last time still stands"*.

### What landed

**Kernel — `crates/iridium-editor/src/fuzzy/path.rs`** (+ `path_tests.rs`,
14 tests). `match_path(&Query, &str) -> Option<PathMatch>`: a *field chooser*
over the existing scorer, exactly the shape `commands/palette/matcher.rs`
has. `PathField::Name` at weight 100, `PathField::Path` at 70, best field
wins, ties to the name.

**The one subtle thing in it:** `PathMatch::matched_in_name()` always returns
positions **into the basename**, whatever field won — path-field positions
are rebased by the basename's character offset and the ones that fall above
it are *dropped, not clamped*. Clamping would pile several matched characters
onto the name's first glyph. The panel draws only the basename, so a caller
never has to know which field matched. This also closes #52's step 2 (the
path-aware entry point).

**`iridium-explorer` gained two read-only accessors:**

- `FileTree::listed_children(node) -> &[NodeId]` — **posts nothing.**
  `TreeSource::children` is the frame path and requests what it lacks, which
  is right for a node someone opened and *wrong for anything that walks*.
- `FileTree::parent(node)` — the chain the reveal path climbs.

**Desktop — `file_tree/` is now six files + a `tests/` directory**, all under
the bar: `panel` (state) · `keys` (what a key does) · `compose` (what reaches
the screen) · `filter` (the walk) · `rows` (one row) · `mod`. Same seam as
the app split: *which question does a defect in this file present as*.

**`highlighted_spans` and `match_color` moved from `command_palette.rs` into
`line.rs`.** Two panels underlining matches through two implementations is
the drift this codebase keeps warning about.

### The three decisions in the filter, and why

1. **Folders kept as context are not selectable.** `FilterRow::score` is
   `None` for them, and that *is* the selectable flag. The arrows step
   between matches and skip the scenery, so they never land somewhere `Enter`
   has nothing to do.
2. **`Enter` on a matched folder reveals it** — drops the query, opens the
   tree down to it, selects it. Safe to expand every step without the
   deferred-intent dance, because **every ancestor of anything the filter
   found is already listed** (a node is in the arena because it appeared in
   its parent's listing). The `open_row` guard is kept anyway: one expansion
   path in the module, not two.
3. **A read landing under a query does not move the selection.** `poll`
   re-filters holding the selected `NodeId`; only if it is gone does the
   selection fall back to the best match. A background read is not a reason
   to open a different file.

### STEP 3B — THE CRAWL, landed 6 Aug, `3c269fa`

3a's filter could match only what was in the arena, and only what someone had
opened was there. A query now drives a crawl, so searching reaches the whole
project.

- **`FileTree::crawl(budget) -> usize`**, called from `FileExplorer::poll`
  with `CRAWL_PER_FRAME = 64` while the query is non-empty. Frontier is a
  `VecDeque`, so **breadth-first** — depth first disappears into the first
  deep branch and finds `src/main.rs` after ten thousand generated ones.
- **`CRAWL_LIMIT = 20_000` directories**, with `crawl_hit_its_limit()` so a
  face can *say* the search was truncated.
- **`FileExplorer::is_waiting()` now covers the crawl too.** It had to: the
  frame that drains the last outstanding read still has a queue, and if only
  `pending > 0` asked for another frame the crawl would stall there. One
  concept — "more is coming".
- **`crates/iridium-explorer/src/ignores.rs`** — nested `.gitignore`,
  deepest-first, each directory's rules compiled against **their own base**
  (an anchored `/build` means "in this directory"; one root-anchored matcher
  gets that wrong *silently*). Plus `.git/info/exclude` and `.git` itself.
- **The rules bind the crawl and nothing else.** A folder opened by hand is
  read and shown whatever git thinks of it. Tested on both sides.
- The empty row says **three** things: `Searching…` while directories are
  outstanding, the limit notice, then `No matching files`. Saying the last
  one early is a lie that resolves itself a second later — long enough for
  someone to have given up.

**Dependency taken, deliberately:** `ignore` 0.4.30 (ripgrep's matcher), in
`[workspace.dependencies]` with the reasoning written in the manifest. Only
`gitignore::{Gitignore, GitignoreBuilder}`, never its walker. The argument is
that gitignore semantics fail *invisibly* when subtly wrong — a file quietly
missing from a search, which nobody reports. Told to Tom as a decision I made
rather than asked about.

### STEP 3C — THE ROOT, landed 6 Aug — the bug Tom found in two minutes

He opened the reinstalled app from the Applications folder and reported that
"what it's trying to index is like the whole system". **He was right, and it
was mine.** `app/host_commands.rs::explorer_root` fell back to
`std::env::current_dir()`, and a macOS bundle launched from Finder or
Spotlight inherits **`/`** from LaunchServices. Harmless while the panel only
listed what you opened; loud the moment `3c269fa` gave a query a crawl.

**The fix separates two questions that had been one:** *where to start* and
*whether a search may read past what is open*. A crawl is worth running only
where something bounds it — inside a repository `.gitignore` does, and it
finishes. A home directory has no ignore rules and its first twenty thousand
directories are application support. **A crawl that cannot work is not worth
starting.**

- **`apps/iridium-desktop/src/project.rs`** — `explorer_root(active_file,
  working_directory, home) -> ExplorerRoot { path, crawl }`. **Pure**: every
  input injected, because the failing case *cannot be reproduced from a
  terminal at all* — a shell always has a sensible `cwd`, so nothing that read
  the environment itself could ever have caught this.
- The `/` test is **`path.parent().is_none()`**, not a comparison against
  `"/"` — right on every platform, and right for a Windows drive root without
  knowing what one is.
- `project_root` walks **up to the nearest** `.git`, so a file in a submodule
  roots at the submodule.
- **`FileExplorer::open(root, crawl)`**, and the flag gates three things —
  `poll`'s crawl call, `is_waiting`, and the empty-list message. All three
  were verified to fail the new test independently. `is_waiting` is the
  non-obvious one: a panel that will never crawl **still has a frontier** (the
  root's subdirectories were queued when interned), so an ungated
  `is_fully_crawled` answers `false` forever and the window repaints at full
  rate for as long as a query is in the field.
- The empty row now says **four** things; the crawl-off answer is
  `No matches in what is open`, and it comes **first**, which is what keeps
  the other three safe to write in terms of the crawl's own state.

**The cost, stated to Tom rather than left to be discovered:** open the app
cold with no file and searching covers only what has been expanded, until a
file or a project is opened.

**Also split, because the fix pushed it over the bar:**
`file_tree/tests/filter.rs` (551) → `filter.rs` (what a query does to the
rows) + `crawl.rs` (what a query causes to be *read*), with the shared
`project()` fixture hoisted into `support.rs`.

### STEP 4 — REGEX, landed 7 Aug

Tom's ruling was **a sigil, not a mode key**: plain text is fuzzy, a leading
`/` makes the rest a regular expression. Built as
**`crates/iridium-editor/src/pattern/`** — in the kernel for the reason
`fuzzy` is there, that a second implementation is a bug waiting to be filed as
a feel problem. Every decision below is invisible until two faces disagree.

- **`Pattern::parse(text)`** → `Unfiltered` · `Fuzzy(Box<Query>)` ·
  `Regex(Box<Regex>)` · `Invalid(String)`. Never fails.
- **Two flags, not one, and keeping them apart is the whole lesson of the
  root bug repeating itself.** `is_narrowing()` — do the rows come from the
  filter — is **true** for `Invalid`, so a half-typed `/[` does not flash the
  whole tree back for one keystroke. `can_match()` — is this worth reading
  the disk for — is **false** for it, because every directory read on behalf
  of a pattern that cannot compile is discarded by construction. The panel's
  crawl gate and `is_waiting` both moved onto `can_match`.
- **A bare `/` is not a filter.** An empty regular expression matches every
  string, so a sigil treated as a pattern would flatten the tree and set the
  crawl going the instant the key was pressed. Pinned by a test in both
  halves.
- **Name first, then the whole path**, mirroring `PathField`'s weights, so
  `^mod` and `widgets/.*[.]rs` both do the obvious thing. A name hit scores
  100, a path hit 70, which puts the selection on the file that is named what
  you typed rather than the first file inside a folder that is.
- **Case-insensitive, always** — the fuzzy half folds case, and a query that
  found a file then lost it because the user reached for `/` is indefensible.
  `(?-i)` is the escape, which is standard syntax rather than an invention.
  **Deliberately not smart-case:** the obvious test — "does the pattern
  contain an uppercase letter" — is wrong for `\S`, `\D`, `\W`, and doing it
  properly means parsing the regex AST.
- **The pattern is held on the panel, not parsed per filter.** Compiling
  costs orders of magnitude more than matching, and the rows re-filter on
  every landing read — dozens a second under a crawl. `requery()` is the only
  place it is rebuilt.
- The empty row now says **five** things; `Bad pattern — <reason>` is first,
  for the same reason the crawl-off answer is: neither state ever drains the
  frontier, so anything checked before them would say `Searching…` forever.
- The reason is lifted out of `regex`'s multi-line message (heading, pattern,
  caret diagram, `error: …`) with a whole-message fallback — ugly and never
  wrong, which is the right way round.

**Also:** `fuzzy::basename_start` and `fuzzy::Basename` are now public, since
`pattern` needs the same answer for the same reason and two implementations
would drift on trailing separators and on `\`.

Tests: 25 in the kernel, 9 in the panel. Three claims verified red first —
bare `/` not filtering, an invalid pattern not crawling, and the message
order.

### STEP 4B — RE-ROOTING, landed 7 Aug — Tom's "one layer deep" report

> "is there a way to sort of set a project root… when you're searching it
> doesn't seem to go more than one layer deep"

**Two reports, one cause, one fix.** The crawl was never depth-limited — a
five-level fixture test proves it reaches the bottom, and a matching one
proves regex does too. Both were added because *every crawl test before them
used a two-level fixture*, so a crawl managing exactly one round of the
frontier would have passed the entire suite. That is the gap that let the
report be plausible.

What Tom actually hit was **yesterday's crawl-off rule**: cold launch, no
file, root at home, `crawl: false`, so searching covers only the root's own
listing — which from the inside is indistinguishable from "one layer deep".
The safety rule read as broken software, which is the signal that the trade
was wrong as shipped.

**The fix is his other request.** `⌘↓` roots at the selected folder, `⌘↑` at
the folder above; `Ctrl+↓`/`Ctrl+↑` for a keyboard with no command key.
macOS's own bindings for "open this folder" and "enclosing folder", so
nothing to learn.

- **`project::chosen_root(path)`** — a chosen directory **earns the crawl**.
  That is the whole difference from `explorer_root`: a directory *this code
  guessed* is not worth reading past, and a person who walked into one has
  bounded it by walking into it. The filesystem root stays the exception —
  nobody navigates there on purpose and no ignore file bounds it.
- **`reroot` builds a whole new arena and reader thread** rather than
  re-projecting the tree from a node inside the old one. `NodeId` is an index
  into *this* arena, so every id the panel holds belongs to the tree being
  replaced; and the ignore rules were compiled against the old root, so a
  re-projection would keep applying a `.gitignore` from above a place the
  user has left. Going up costs what going down costs, and that uniformity is
  worth more than the milliseconds.
- **`ExplorerOutcome::Failed(String)`** — the panel is modal, so a key that
  was consumed and did nothing is indistinguishable from a dead one. The host
  puts it on the prompt strip and leaves the panel open.
- **Deliberately not `Enter`.** That is still Tom's open question; binding
  these separately means whatever he rules only *adds* a way in.

### The query field is deliberately caretless

It takes printable characters, `Backspace` and paste — nothing else. `←`/`→`
in a tree panel belong to the tree, and a filter is three or four characters
someone retypes rather than edits. `Escape` clears the query first and closes
the panel second.

## ⚙️ CONFIGURATION — landed 7 Aug, `b371431` + the desktop wiring

Tom asked for "settings things and a way to set the keymaps"; he ruled **file
first, UI over it later**. The file half is done and wired into the desktop
face. `docs/CONFIG.md` is the user-facing reference — that is what to send him.

```text
~/.config/iridium/config.toml     # or $XDG_CONFIG_HOME/iridium/config.toml

[editor]
tab_width = 2

[keys]
"cmd+shift+p" = "palette.open"
"ctrl+f"      = ""                # empty command unbinds
```

### The kernel half already existed

This was the finding that shaped the work. `EditorConfig` already derived
serde; `KeyBinding::parse`'s own doc already called itself *"the text form a
configuration file or a rebinding UI works in"*; `push_validated_keymap` was
already documented as *"the path a host loading a keymap from configuration
should take"*. So the work was location, reading, and **reporting** — not
keymap machinery.

### `crates/iridium-config` — native-only, nine files, 59 tests

Native-only for the reason `iridium-explorer` and `iridium-file` are: **a
browser has no configuration file**, so the wasm bundle must never carry a TOML
parser. What can genuinely drift between faces is already in the kernel.

Four rules, in priority order:

1. **A bad configuration never stops the editor.** Nothing returns a fatal
   error. The reason is circular and decisive: the usual way to fix a
   configuration file is to open it in the editor.
2. **A mistake costs its own line.** Sections are read separately, and inside
   `[editor]` *every setting is applied separately*. Only a whole-file syntax
   error takes everything, because it leaves no sections to isolate.
3. **A refused binding is reported, never swallowed** — see below.
4. **No file is not a problem.**

### Two things taken from the source of truth, not copied beside it

- **Which settings exist** is read from `EditorConfig`'s own serde derive, via
  the field names it hands `deserialize_struct` (`fields.rs`). A hand-written
  list would report a newly added setting as a typo — confidently wrong rather
  than merely silent. A test asserts the capture still answers, so introducing
  `#[serde(flatten)]` fails loudly instead of switching the check off.
- **Whether a value is acceptable** is decided by deserializing it against the
  real type, never by a description of what the type accepts.

`EditorConfig` gained `#[serde(default)]` on the container so a partial
document is valid. Strictly more permissive; nothing that parsed before stops.

### `install` is a loop, and that is the whole point

The kernel validates a layer **whole** and refuses it whole — correct for the
kernel, but one mistyped command id would cost every binding in the file. So
`install` reads the refusal, drops the binding it names, records why, and
offers the rest again. Identification is a comparison, not a guess: the
kernel's errors carry `display_sequence()` output and so does the binding.

**It never suppresses a stranded default chord on the user's behalf.** Binding
a bare `ctrl+f` strands every `ctrl+f …` sequence; auto-unbinding them would
mean an unrelated line silently switching off a key. The new binding is dropped
and the report quotes the exact line that would keep it.

### The proxy the unit tests could not close

`iridium-config`'s own tests build `KeymapError`s by hand — a **proxy** for
what the kernel really produces. The divergence case: the error's text and the
binding's `display_sequence()` stop agreeing. So
`app/config.rs` tests against a **real `Workspace`** with the real default
keymap, and one of them (`ctrl+f x`, stranded by the default's bare `ctrl+f`)
can only pass if the two spellings genuinely match.

### Reporting: a tab, not just the strip

The strip holds one line, and this face is launched from Finder where stderr
goes nowhere. So problems open a tab called **`config.toml`**, *behind* the
file that was asked for, and the strip says the tab is there.

### `commands.list` — you cannot bind what you cannot name

The palette matches on ids but **shows titles**, so there was no way to find
out what to write in `[keys]`. New palette-only command **"List Every
Command"** opens a tab of `id · title · key` from the live registry and hint
index. Deliberately unbound, recorded on a `PALETTE_ONLY` list in
`commands.rs` with a second test asserting the list never goes stale.

### Left for the UI half

A settings *panel* over this file. Also unbuilt: reloading the file without a
restart, and a "create a starter config.toml" affordance.

---

## 🔤 ONE `Language` TYPE — landed 7 Aug, and what a lint sweep uncovered

Task #60 was meant to be tidying. The gate is armed `--all-features`, feature
unification turns everything on, so **the parser-free kernel was never linted**
— 17 violations sat there. Reading them found the real thing.

### There were two `Language` enums, and they disagreed

The real one in `iridium-syntax`, and a hand-maintained stub in
`iridium-editor/src/syntax_stubs.rs`, chosen by the `syntax` feature.

| | real | stub |
|---|---|---|
| `from_id` aliases | `rs`, `py`, `golang`, `zsh`, `c++`… | none |
| `from_id` case | lower-cased | case-**sensitive** |
| `from_extension` | knows `.pyw` | does not |
| `from_extension` | no `.jsonc` | has `.jsonc` |
| `all()` | `const fn -> &[Self; COUNT]` | `fn -> &[Language]`, no `COUNT` |
| serde | derives both | neither |

So `Language::from_id("rs")` was `Some(Rust)` in one build and `None` in the
other.

**No test could ever have caught it.** The two are feature *alternatives* —
`syntax_stubs` is itself `#[cfg(not(feature = "syntax"))]`, so no build has
both in scope and nothing can compare them. A guard was structurally
impossible, which is why the answer is **one type**, not two copies and a
check.

### The live defect it had already caused

`comments.rs` split `language_tokens` on the same feature, and the stub
returned "no comment syntax" for **every** language — justified by a comment
saying the path was unreachable because "`Language::from_id` always returns
`None`". That was simply false; the stub's `from_id` resolved every canonical
id. So in the parser-free kernel `Ctrl+/` on a Rust file inserted nothing, or
fell through to `EditorConfig::line_comment_token` — a `#` in a Rust file,
which reads as a decision somebody made.

Latent, not shipping: every face enables `syntax`. But `--no-default-features`
is gated, tested, and what the workspace dependency line defaults to.

Red-test-first held: un-gating `mod language_table` gave **15 failures**
against the unfixed code, then 41 passes after.

### The fix

**`crates/iridium-lang`** — the enum, `COUNT`, `id`, `from_id`,
`from_extension`, `all`, `index`, serde. No dependency that parses anything;
that is the whole reason it can be shared. `iridium-syntax` re-exports it,
`iridium-editor` depends on it **unconditionally**, and the stub is gone.

### The rule this leaves behind

`syntax_stubs.rs` now says it: **if a stub could answer a question correctly,
it should not be a stub.** Everything left in that file genuinely needs
tree-sitter to exist.

Three lints were *wrong* and are answered with `#[expect(..., reason = ...)]`
rather than a change, all for one reason: the stub `Tree` is zero-sized and
`Copy`, the real one is neither, so "pass by value" and "use `copied()`" would
compile feature-off and fail feature-on. One needed `#[cfg_attr(not(feature =
"syntax"), expect(...))]`, because `#[expect]` is strict in both directions and
an unfulfilled expectation is itself an error.

### Left open — decide, do not drift

- **`.h` is C, not C++** — asserted in a test so changing it is a decision.

---

## 🗒 `.jsonc` — SETTLED 7 Aug, verified against the grammar

The stub had taken `.jsonc`, the real table had not, and collapsing the two took
the real one's answer. That left `.jsonc` resolving to no language at all. The
stub turned out to have been right.

**What was checked, not guessed.** tree-sitter-json 0.24.8 lists `comment` in
its `extras` (grammar.js:16, rule at :94, covering `//` and `/* */`), and the
vendored `json/highlights.scm` already captures `(comment) @comment`. A
commented document parses with **`has_error = false`** — no recovery, no error
node — and both comment forms highlight, covering exactly their own text. Two
tests in `iridium-syntax/src/highlight/tests.rs` pin that against a grammar
bump: `jsonc_is_the_json_grammar_and_its_comments_parse` and
`a_trailing_comma_costs_an_error_node_and_nothing_else`.

**Trailing commas are the other half of JSONC and the grammar does reject
them** — but the damage is *local*. Recovery marks one `MISSING` value and one
`ERROR` node and carries on; every real token either side still highlights as
itself, so nothing is mis-coloured. Wrong highlighting would have been worse
than none; this is neither.

**What this does not buy.** `Ctrl+/` in a `.jsonc` file is still a no-op,
because `language_tokens(Json)` is `(None, None)` in `iridium-editor` and the
config fallback defaults to `None`. Giving JSON a `//` token would give every
`.json` file one too — commenting a line in `package.json` would silently break
it. The two formats share a grammar but not a specification, and separating
them means a `Language::Jsonc` variant, which collides with
`distinct_languages_do_not_share_one_grammar` in `grammar.rs`. **That is Tom's
call, not mine** — it is task #62, priced there.

### The proxy this uncovered

`get_extensions_for_language` in `iridium-bindings/src/lib.rs` was a
**hand-written inverse** of `from_extension` — a second copy of one decision,
and it went stale the instant `.jsonc` was added. Same shape as the two
`Language` enums.

The fix is the non-proxy one: `Language::extensions()` is now the **only**
extension table, and `from_extension` *searches* it rather than restating it, so
the two directions cannot disagree. The napi function is four lines reading that
table. Three guards were added in `iridium-lang`:

- `every_listed_extension_resolves_to_the_language_that_lists_it`
- `no_extension_names_two_languages` — this is what makes the linear scan's
  order irrelevant, asserted rather than assumed
- `every_extension_is_lower_case_and_carries_no_dot` — an upper-case entry would
  be unreachable, since `from_extension` lower-cases first

The scan is a few dozen string comparisons on the path that opens a file, not on
any path that runs per keystroke or per frame.

---

## 🧪 ONE GPU TEST HARNESS — landed 7 Aug, `3985823`

`top_inset.rs`, `left_inset.rs` and `retained_shaping.rs` each carried their
own copy of the headless harness. Now `tests/support/` — `mod.rs` (declarations
only) plus `gpu.rs`, `frame.rs`, `pixels.rs`, `scene.rs`.

**The copies had already drifted, in the way that mattered most.** Two of the
three passed `|_| {}` as the `map_async` callback and threw the result away, so
a failed readback reached `get_mapped_range` with nothing having said why — it
would have surfaced as a mysterious *geometry* failure. Only `retained_shaping`
checked. **The shared version is that one.**

Two more one-decision-written-twice instances closed:

- `top_inset` documented at length why the top-left corner is exactly the wrong
  reference for "page" under a top inset — and then sampled the top-left anyway,
  two functions further down. Latent (both corners are page on a correct frame),
  same shape. One `page()` now.
- The ink threshold existed **four** times: named `INK` twice, and spelled as a
  bare `24` inline in two of `top_inset`'s band loops.

**Deliberately not merged:** `retained_shaping`'s `NoHighlights` answers `true`
to `language_active` where the inset harnesses' answers `false`, and that
difference is the subject of several of its tests. Renamed
`ActiveLanguageNoSpans` so two behaviours no longer share one name.

Two rules worth keeping:

- **`dead_code` is `allow`, not `expect`, in a test support module.** It
  compiles once per test binary and each uses a different subset, so `#[expect]`
  would fire as *unfulfilled* in whichever binary used everything.
- **Do not answer a cast lint with another cast.** `HEIGHT_F32` is derived from
  `HEIGHT`; `cast_precision_loss` is answered by a const assertion that `HEIGHT
  < 2^24` — the actual condition — because casting back trips two further lints.

666 → 447 and 596 → 400, both under the bar. `retained_shaping` 1,133 → 981,
still over; splitting it is **#65**.

---

## 🩹 TWO BOUNDARY FIXES — landed 7 Aug, `44f85da` and `ccf4771`

### A document can stop having a language (#39)

`Language` has thirteen variants and every one names a grammar, so there was
no way back to plain text. **Save As** is where that was live — the one path
where a document changes what it is called without changing which editor holds
it. `a.rs` saved as `notes.log` stayed Rust, in both faces.

`EditorState::clear_language` clears **three** holders, and the third is the
point: fold state, parse tree, and an id string on the document.
`Editor::language` reports the first; **comment toggling reads the third.**
Clearing only what `language()` can see gives a document that reports no
language and still inserts `//`. Proven — with that one line removed the test
fails with `left: "// fn main() {}"`.

**#39's stated symptom was stale.** It said drag-and-drop left the previous
file's language in force; `open_file` has since become open-in-a-new-tab and
its own doc comment already says that class cannot arise. The kernel gap was
real, the path had moved. *Check the code before repeating a claim from a task.*

### A JavaScript number is checked before it becomes an index (#40)

Five sites in `wasm.rs` narrowed an `f64` from JS with a bare `as usize`.
**`as` saturates**: `-1.0` → `0`, `NaN` → `0`, `1e30` → `usize::MAX`. So a bad
line number was not an error — it drew on **line zero**, a real line, wrong,
and the message named nothing.

The policy lives in `crate::js_index`, which **compiles for the host and is
tested there**. That split is the whole design: nothing in `wasm.rs` is
reachable by a native test, so a check written inline is a check nothing can
exercise. Refused: negative, `NaN`, both infinities, past 2^53, and fractions.

**Deliberately not refused: a valid line past the end of the document.** That
is a race, not a mistake — a host computes spans against a revision that has
since moved, which is ordinary, and the renderer already draws nothing for
them. Erroring would make normal operation fail.

---

## ⏸ WAITING ON TOM (nothing is blocked *behind* these)

### ✅ RULED 7 Aug ~15:27 — TOM GAVE THE GO-AHEAD, TAKE THE RECOMMENDATIONS

*"You've got the go ahead to go do everything you need to do there. I'll take
your recommendations. You can find the AWL grammar. You might need to message
**Vesper Lynd** — they are responsible for **Aeon**, so they will have all the
answers you need."*

So the recommendations are now the decisions. **Nothing below is still waiting
on him.**

- **L-0 → Tier 1 now.** Nothing hard-coded; grammars vendored, queries and
  per-language behaviour as data. No wasmtime, no 6.6 MiB. Tier 2 stays a
  later, strictly additive step.
- **#66 crate question → (b).** A new crate owns the vendored `languages/`
  tree; `iridium-lang` and `iridium-syntax` both depend on it. Nothing reaches
  backwards, one owner for one directory. **Consider folding `iridium-lang`
  into it** — the registry step has to answer what that crate is for anyway.
- **#62 → answered by data.** Reading the manifests gives JSON `line_comments
  = ["// "]`, so `Ctrl+/` starts working in `.jsonc` and `.json`. Zed's call,
  overridable in a file. Close #62 when #66 lands.
- **LSP → after languages**, so the server registry falls out of the language
  registry rather than being built twice.
- **AWL → ask Vesper Lynd.** They own **Aeon**. Needed: is the tree-sitter
  package public or internal, and does it ship `highlights.scm` and friends or
  only the grammar? Send with the Meridian `send` tool using `to: "Vesper
  Lynd"` — this seat has no DM id for them yet.

**IN FLIGHT, ASKED 7 Aug ~15:30, NOW ANSWERED:** #66 is started and was parked
one question short. The 21 vendored `config.toml` manifests reproduce the whole comment
table exactly — see `docs/IN-FLIGHT-manifests.md`, which is complete and has
the derivation, the `documentation_comment` fallback that four languages
depend on, and the equality oracle to write. **The blocker:** those manifests
live in `iridium-syntax`, the *optional* crate, and `comments.rs` must work
with `syntax` off — reading them from there re-creates the exact defect #60
removed. Three ways out priced in that file; **I recommended (b), a crate that
owns the vendored tree outright.** Awaiting Tom's nod because it is workspace
shape, not a detail in a file. **Nothing is half-written — no code was
started.**

| # | question |
|---|---|
| **#66** | **(a) move the tree to `iridium-lang`, (b) a new crate owns it, or (c) split it?** Recommended (b). |
| **#63** | **L-0: which extensibility tier.** Tier 1 free, tier 2 measured at 6.6 MiB + 90 crates. Also: is AWL's grammar public or internal, and does it ship `.scm` files? |
| **#63** | LSP: now or after languages? And depend on chiron's `lsp` crate or vendor it? |
| #62 | Should `Ctrl+/` write a comment in a `.jsonc` file? Three options priced. |
| ~~#58~~ | ~~`Enter` on a folder — descend-and-re-root, or toggle?~~ **RULED 7 Aug: toggle.** |
| ~~—~~ | ~~Does the explorer replace ⌘O or sit beside it?~~ **RULED 7 Aug: it takes the chord. ⌘O was never bound.** |
| — | The inactive tab close control's alpha (0.40); the wheel over the tab strip. |

---

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
cargo clippy -p iridium-editor --no-default-features --all-targets -- -D warnings
cargo clippy -p iridium-editor --no-default-features --features syntax --all-targets -- -D warnings
cargo fmt --all --check
```

**Eight, not six — the last two were added 7 Aug and here is why.**
`--all-features` unifies every feature *on*, so the clippy line above it never
sees the parser-free kernel at all. Seventeen violations had accumulated there
unseen, and looking at them turned up a live defect: two `Language` enums that
disagreed, and a comment-syntax table gated on the same feature, so `Ctrl+/`
did nothing in that build. **A configuration nothing lints is a configuration
nothing is checking.**

The screenshot harness, when a frame needs judging by eye:

```
IRIDIUM_CHROME_SHOT_DIR=<dir> cargo test -p iridium-desktop \
    --test chrome_screenshots -- --ignored
```

**The lesson that keeps earning its keep:** *the proxy law is not satisfied by
testing the queries. Render it and look at it* — and then check that what you
looked at was the thing you meant.

---

## Extensible language support — DONE (#63)

Three commits, and the shape is the point:

- `6f58c72` — `Language` stops being a closed enum. It is a newtype over a
  table generated by `build.rs` from a hand-edited `languages.txt`. Only five
  sites depended on the enum being closed, and ~200 call sites were preserved
  unedited by generating associated constants with the same spelling.
- `6da7b89` — AWL vendored. **Cost of adding a language, after the registry: a
  directory of query files, one line of text, four lines declaring a C symbol.**
- `1f1345c` — the four grammarless languages. `grammar()` returning `None` is
  legal, so `diff`, `gitcommit`, `gomod` and `gowork` claim files and carry
  comment tokens with no parser at all.

**⚠️ `cargo test` is fail-fast at the TARGET level.** A run that stops early
reads exactly like a run that got further than it did — one workspace run here
reported a single failure having never reached `iridium-syntax`, which held
three more. `--no-fail-fast` is now in the battery script for every test gate.

**⚠️ The background-task notification's exit code lied twice** in this stint,
reporting 0 for runs that exited 101. Redirect to a log, record `$?` on the
very next line, and read the status file.

## Open questions and known gaps

- **Awaiting Tom**: should a new file open into the current group or always at
  the top level? Defaulting to the current group — one line to change
  (`open_file` in `app.rs`, the `parent` binding).
- **Awaiting Tom (asked 6 Aug)**: the inactive close control's alpha, and
  whether the wheel over the strip should scroll the strip.
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
- **#70** nine tree-sitter capture names map to no `HighlightType`, so the
  tokens render in the plain foreground with nothing reporting it: markdown
  headings and links, *every* CSS selector, JSX tags. Found by a sweep, not by
  anyone noticing the colour. Ratcheted by `KNOWN_UNSTYLED_GAP` in
  `iridium-syntax/src/highlight/tests.rs` so it cannot grow. `@namespace` —
  which hit `awl`, `cpp`, `css` and `go` — was fixed at the same time.
- **#69** `custom_gutter_lines_miss_and_recompose_identically` is flaky under
  box load. Passed in the AWL battery; do not read one green run as a fix.

## 🧪 THE RETAINED-SHAPING SUITE SPLIT — #65

`crates/iridium-editor/tests/retained_shaping.rs` had grown to **1,062 lines**
against the 500-line bar. It is now `tests/retained_shaping/` — one test
binary still, because cargo takes `tests/<name>/main.rs` as a target, so no
link time was added and no test was renamed.

**The seam is which input of the shape key the row mutates**, which is the
axis a defect appears along: a compositor that stopped keying the fold
generation breaks the fold rows and nothing else.

| file | lines | what it holds |
| --- | --- | --- |
| `main.rs` | 48 | the module doc and the declarations, nothing else |
| `harness.rs` | 182 | the fixtures, and the single path every comparand is built by |
| `hits.rs` | 140 | the inputs deliberately outside the key |
| `typing.rs` | 176 | the miss side, stage 2a — the edit path |
| `layout.rs` | 140 | scroll, target size, font face, font size |
| `theme.rs` | 87 | the palette text and the fallback highlighter draw from |
| `syntax.rs` | 217 | the language answer, the spans, their generation |
| `gutter.rs` | 157 | folds, custom gutter lines, the digit rollover |

**All 24 tests kept, none renamed, none rewritten — and that is checked, not
claimed.** Concatenating the six modules from each one's first doc comment and
diffing against `git show HEAD:…/retained_shaping.rs` lines 199–1062, with only
the three now-redundant section banners removed, comes back **identical**. A
green suite would not have proved this: a split that quietly weakened an
assertion passes just as well.
`support/` is reached with `#[path = "../support/mod.rs"]`, which leaves the
other two GPU binaries untouched.

⚠️ The flaky `custom_gutter_lines_miss_and_recompose_identically` (#69) is now
in `gutter.rs`. Splitting did not touch it.

## 🔬 THE PIXEL ASSERTIONS NOW SAY *HOW* THEY DIFFER — #69 groundwork

`custom_gutter_lines_miss_and_recompose_identically` is flaky under box load.
It did **not** reproduce in 12 consecutive runs on an idle box, which matches
the report and means the useful move was not to chase it.

Every one of the 25 pixel-identity comparisons in the retained-shaping suite
was `assert!(a == b, "the X frame must be byte-identical")` — a bare bool. A
glyph edge off by one and a frame composed from the wrong document produced
**the same message**, so when this flake fired it left nothing behind to work
from.

`support::pixels::assert_same_frame` replaces all 25. On failure it now says:

```
⚠️ EXAMPLE ONLY — INVENTED NUMBERS, NOT A CAPTURED RUN. See the marker below.
the custom-gutter frame must be byte-identical
  1 of 393216 pixels differ; the first at column 3, row 2; they span
  columns 3..=3 and rows 2..=2; the largest single channel difference is 23
```

⛔⛔ **MARKED 8 Aug 2026 — THIS BLOCK WAS LATER CITED AS AN OBSERVATION AND IT IS
NOT ONE.** It was hand-written here to demonstrate the message format; nothing
was running. `docs/IN-FLIGHT-69-flaky-gutter.md` went on to quote it as *"the
one occurrence on record"* and reasoned from it. ⭐ **The population is what
gives it away: every readback target in the suite is 512 × 384 = 196,608
pixels, so no run of this test can print 393,216.** #69 has, and has always
had, **no recorded signature** — see `docs/SESSION-STATE.md` above at the
original report, which records only "flaky under box load".

⭐ **An illustration placed in a record is indistinguishable from a measurement
once the surrounding prose is gone.** The framing words "it now says" do not
survive being quoted. Mark invented numbers *at the numbers*, never only in the
sentence above them.

Count, position, area and magnitude — which is the difference between "the
glyph atlas packed differently" and "this is a regression". The message is
only formatted on failure, so the scan costs nothing on the passing path.

Four pure tests pin the helper itself, including that it reports the
magnitude — a diagnostic nobody has proved is a diagnostic nobody should
trust.

**This is not a fix for #69.** It is what makes the next occurrence
diagnosable instead of another "it differed".

---

## #71 — the FrameTimer clock seam

**Status: compiled, tested, three gates green, five gates outstanding.**
Written during a window when the box would not permit a build; verified in a
later window when it briefly would. The section below was written before the
first compile and is kept as it stood — the verification results are recorded
at the end of it.

### What is on disk

`crates/iridium-editor/src/view/frame_timer.rs` (506 lines, already over the
500 bar) was **deleted** and replaced by a directory:

| file | lines | holds |
| --- | --- | --- |
| `frame_timer/mod.rs` | 35 | declarations and re-exports only |
| `frame_timer/budget.rs` | 61 | `FrameBudget`, `FrameStats`, the thresholds, `TARGET_*` |
| `frame_timer/timer.rs` | ~330 | `FrameTimer` |
| `frame_timer/delta.rs` | ~94 | `DeltaTime` |
| `frame_timer/timer_tests.rs` | ~272 | timer tests |
| `frame_timer/delta_tests.rs` | ~114 | delta tests |

The public API through `view/mod.rs` and `lib.rs:95` is **unchanged**, so no
re-export edits were needed. `FrameTimer` and `DeltaTime` have **no callers
anywhere in the tree** — only those two re-export lines — so the API was free
to change without breaking anything.

### What the change is

Every clock-dependent method gained an `_at(now: Instant)` twin, with the
wall-clock method reduced to a one-line wrapper over it. `new_at`,
`with_target_fps_at`, `begin_frame_at`, `end_frame_at`, `elapsed_at`,
`remaining_at`, `time_until_next_frame_at`, `current_budget_at`,
`has_budget_for_optional_work_at`; and on `DeltaTime`, `new_at` and
`update_at`. This is the seam `render::cursor` already has.

Six tests that asserted **upper bounds on wall-clock elapsed** were converted
to synthetic instants and, in every case, strengthened from a bound to an
equality. See `docs/IN-FLIGHT-box-gate.md` for the audit that found them.

### Three defects found while doing it

1. **`with_target_fps(0)` panicked.** `1.0 / 0.0` is infinite and
   `Duration::from_secs_f64` panics on a non-finite value. A public
   constructor could be made to panic by its argument. The divisor is now
   floored at one, and `a_zero_target_fps_is_floored_rather_than_panicking`
   pins it.
2. **`delta_time_caps` did not test the cap.** It set a 50 ms maximum, slept
   60 ms, and asserted `delta() <= 60ms` — a bound *above* the 50 ms the cap
   should produce. Whether it caught a missing cap depended on `thread::sleep`
   overshooting, which is to say on box load. Against a synthetic clock a
   missing cap yields exactly 60 ms and **the old assertion passes outright**.
   Now `assert_eq!(dt.delta(), 50ms)`.
3. **Three of the four `FrameBudget` bands were untested**, because reaching
   them meant sleeping through 7 ms and 8 ms thresholds. All four bands and
   both sides of every boundary are now named exactly.

### The gap I deliberately left, and covered

Every `_at` test would still pass if a convenience wrapper stopped reading the
clock — if `begin_frame` never called `begin_frame_at`. Two tests exist purely
to catch that: `the_wall_clock_wrappers_read_the_clock` and
`the_wall_clock_wrapper_reads_the_clock`. They sleep, deliberately, and assert
**lower** bounds only, so load can only make them more true.

### ⛔ What must happen before this is committed *(written pre-compile)*

**It has never been compiled.** Load was 19.28, then 12.44, then 18.78, then
**48.31** against 10 cores across the writing of it, so nothing was built.
Self-review only. Run the full eight-gate battery before this lands, and treat
a first compile as likely to surface something.

Specific things I could not check by compiling. **Three of them have since
been resolved by reading the vendored source**, which needed no build:

| risk | resolved | where |
| --- | --- | --- |
| does `web_time::Instant` have `saturating_duration_since`? | **yes** | `web-time-1.1.0/src/time/instant.rs:55` |
| is `start + Duration::from_millis(..)` valid? | **yes** | `impl Add<Duration> for Instant`, `:76` |
| can `Instant` be passed by value and `now` reused? | **yes** | `#[derive(Clone, Copy, ..)]`, `:14` |

Still unverified, both low risk: whether `pub const fn begin_frame_at(&mut
self, ..)` is accepted — `set_max_delta` already ships as `pub const fn` with
`&mut self`, so this toolchain takes `&mut` in a const fn — and inference of
the loop index as `u32` in `frame_timer_records_frames`, which follows from
`impl Mul<u32> for Duration`.

**None of this substitutes for a compile.** It only means the first one is
less likely to fail on the things I already knew to doubt.

### The load observer

`<scratchpad>/loadsample.sh` is running in the background writing
`<scratchpad>/load.tsv` every 30 s for up to 4 hours. It **records and
compares nothing** — Cally's distinction between a disclosure and a
precondition. Two uses: knowing when the box is affordable, and deriving the
≥300 s debounce from troughs measured *here* rather than inherited from
another box's ~90 s observation.

**I am not writing my own gate.** Cally's argument that 136 hand-rolled
preflights would diverge is right, and the commitment holds in its honest
form instead: batteries stay ungated and declared so, until a shared preflight
exists.

### ✅ Verification, in the 20:53 window

Load fell under the 10-core threshold at 20:53:52. Rather than start a
fifteen-minute battery on a 240 s window — when the widest lull measured on
this box that later *closed* was 601 s — the runs were escalated by cost, on
the reasoning that **a debounce should scale with the duration of the run it
gates**: a long run needs confidence the quiet will persist, a 15 s check
mostly needs it to exist.

| gate | result | cost |
| --- | --- | --- |
| `cargo check -p iridium-editor --all-features --all-targets` | **exit 0**, 14.65 s | load 6.28 → 6.55 |
| `cargo test -p iridium-editor --all-features --lib frame_timer` | **23 passed, 0 failed** | runs in ~0.00 s |
| `cargo clippy -p iridium-editor --all-features --all-targets -- -D warnings` | **exit 0** | — |
| `cargo fmt --all` | **exit 0** | — |

**Every risk flagged before the compile held.** `saturating_duration_since`,
`Add<Duration> for Instant`, `Instant: Copy`, `const fn` with `&mut self`, and
the `u32` inference all compiled first time.

#### Three defects the first runs found, all mine, none in the code

1. **`every_frame_budget_band_is_reachable` was wrong about the Overrun
   edge.** It used a literal `8_333 µs`, but `TARGET_FRAME_DURATION` is
   `from_micros(8333)` while the timer *divides* —
   `from_secs_f64(1.0 / 120.0)` is **8333.333 µs**. The literal sat 333 ns
   below the real edge and landed in `Critical`. Now asks the timer for its
   own target: writing the constant there tests the test's arithmetic, not the
   band.
2. **`time_until_next_frame_counts_from_the_last_frame_end` had my arithmetic
   wrong** — expected 700 µs where the answer is 500 µs, because the wait
   counts from the frame's *end* at +200 µs.
3. **Clippy caught `unchecked_time_subtraction` twice**, on both new `Duration`
   subtractions. Its suggested `checked_sub(..).unwrap()` collides with the
   project's unwrap ban; `saturating_sub` gives the identical value with no
   panic path.

#### Discrimination proven, both fixes

Both were temporarily reverted and the tests confirmed red before restoring:

- `fps.max(1)` → `fps`: `a_zero_target_fps_is_floored_rather_than_panicking`
  **FAILED**, panicking inside `core/src/time.rs:962` — `from_secs_f64`
  rejecting the infinity, exactly as predicted.
- `.min(self.max_delta)` removed: `delta_time_caps` **FAILED**, while
  `an_interval_under_the_cap_passes_through_unchanged` **stayed green** — the
  control that shows `delta_time_caps` is doing the discriminating rather than
  the whole module falling over.

### ⚠️ Five gates still outstanding

Load returned to **11.78** before the battery could run. Not run, and the
commit says so:

```
cargo test --workspace --all-features --no-fail-fast
cargo test -p iridium-editor --no-default-features --no-fail-fast
cargo test -p iridium-editor --no-default-features --features syntax --no-fail-fast
cargo check -p iridium-bindings --no-default-features --features web --target wasm32-unknown-unknown
cargo clippy -p iridium-editor --no-default-features --all-targets -- -D warnings
cargo clippy -p iridium-editor --no-default-features --features syntax --all-targets -- -D warnings
```

Risk is low — `FrameTimer` and `DeltaTime` have **no callers anywhere**, the
public API through `view/mod.rs` and `lib.rs:95` is unchanged, and the module
depends on nothing but `std` and `web_time`. **Low is not zero.** Run these in
the next window that permits them.

### #71 gates: four of six now green

An opportunistic runner caught a window at 21:41 and spent it cheapest-first.
It was killed by the harness partway through — along with the load observer, at
the same moment, which is what makes it a reap rather than anything about the
box — but the ordering meant the window still bought four gates in 51 seconds:

| gate | exit | secs | load before → after |
| --- | --- | --- | --- |
| `cargo fmt --all --check` | **0** | 2 | 9.69 → 9.69 |
| `clippy -p iridium-editor --no-default-features --all-targets -D warnings` | **0** | 12 | 9.69 → 9.22 |
| `clippy … --no-default-features --features syntax --all-targets -D warnings` | **0** | 11 | 9.22 → 8.74 |
| `test -p iridium-editor --no-default-features --no-fail-fast` | **0** | 26 | 8.74 → **11.16** |

The window closed on the fourth gate — `t_kernel` ended above threshold. Had
the expensive gate been ordered first it would have been the only one attempted
and it would have been caught by the close.

**Still outstanding, three:**

```
cargo test -p iridium-editor --no-default-features --features syntax --no-fail-fast
cargo check -p iridium-bindings --no-default-features --features web --target wasm32-unknown-unknown
cargo test --workspace --all-features --no-fail-fast
```

⚠️ **Do not re-launch the runner as a background task.** Both background tasks
were killed; a third attempt would be fighting the harness rather than reading
it. Run the remaining three inline, on a tick where load permits.

#### `fmt_check=0` was verified rather than trusted

That gate wrote **65 KB** while exiting 0, which does not look like a pass. It
is: the entire volume is `rustfmt.toml` declaring options that need nightly,
warned **609 times**. No `Diff in` lines, no mention of the changed files, and
an independent re-run reproduced exit 0 with byte-identical output.

⭐ **609 repetitions of a benign warning is where a real one hides.** A genuine
`Diff in <path>` would be invisible to anyone reading that log; only the
separately-recorded exit status distinguishes pass from fail. Filed as **#73**,
along with the fact that 21 declared formatting options — including
`error_on_line_overflow` and `error_on_unformatted` — do nothing at all.

---

## ▶ PICK UP HERE — state at 22:44, 2026-08-07

### #71 — CLOSED. All six gates green.

The three that were outstanding ran inline at 22:42–22:44, in a window that
opened at 1-minute load **9.16** against 10 cores and kept falling for the whole
battery (9.13 → 8.59 → 7.59). The entire set fit in **100 seconds**, which is
inside three of the four troughs measured earlier this evening — the
escalate-by-cost ordering is what made that fit possible, and it is worth
keeping.

| gate | exit | evidence |
| --- | --- | --- |
| `test -p iridium-editor --features syntax` | 0 | 1109 + 3 + 16 passed, 0 failed |
| `check -p iridium-bindings --features web --target wasm32` | 0 | finished in 3.09 s, **zero** warnings |
| `test --workspace --all-features` | 0 | **2463 passed, 0 failed**, 20 ignored, across 30 targets |

Each ran unpiped with `$?` captured on the very next line into its own status
file, per the standing rule — `t_syntax.status`, `wasm.status`, `t_all.status`
in the scratchpad. The last gate was **split into `--no-run` then run**, so the
compile (26 s) and the execution carried separate exit statuses; a window
closing mid-compile would have cost the compile only, and the compile is
resumable. That split is the right default for the expensive gate on this box.

#71's code is committed at `bd582e1`; that commit's message names four of six
gates because four is what had run at the time. **This is the record that the
other two passed** — the commit was not amended, because amending it to claim
gates it did not run would put a claim in the history that was false when
written.

### #72 — landed, and it was not the small one

All three captures are resolved: `tag.jsx` → `Tag`, `text.jsx` and `nested` →
`DELIBERATELY_UNSTYLED`. `KNOWN_UNSTYLED_GAP` is down from nine names to six,
and those six are exactly #70's colour choices, still Tom's call.

**It was billed as needing only a build. It needed a defect fix in every
language.** Full account in `IN-FLIGHT-span-precedence.md`; the short version:

- The write-up's claim that `<div>` renders unstyled was **wrong**. It rendered
  as a `Type` — the unpredicated `@type` pattern matches lowercase names too,
  and a third pattern captures it as `Variable`. Reading a `.scm` shows which
  patterns exist, not which fire together on one node.
- Chasing that exposed **fifteen byte ranges carrying two highlights** across
  five one-line sources, in TSX, TypeScript, JavaScript and Rust —
  reproduced on **unmodified code**. `fn main()` is one of them: `main` is
  captured as both `FunctionDefinition` and `Variable`.
- Nothing downstream could break the tie. `HighlightSpan`'s `Ord` ignores the
  highlight, so the pair compares equal, and the desktop resolver sorts with
  `sort_unstable` before taking the first claim on each byte. The right colour
  was on screen only because `sort_unstable` happens to preserve order below
  ~20 elements — which is *not* an ordinary line of code.
- Fixed in `spans_with`: stable `sort`, then `dedup_by` on the range alone,
  keeping the first span emitted. By construction it changes no colour that is
  currently stable; it makes today's appearance a guarantee rather than a
  coincidence.

Two other precedence rules were tried and rejected, both recorded with the
reason. Lowest-pattern-index made a token's colour depend on the byte range
being queried — it would have changed colour on scroll.

`highlight.rs` went 478 → 517 lines carrying that reasoning, so it was split on
the `frame_timer/` pattern into `mod.rs` / `capture.rs` / `highlighter.rs` /
`span.rs`, and `tests.rs` — **already** over at 622 — into `tests/mod.rs` /
`capture.rs` / `span.rs`. Everything is under 500 and the public paths are
unchanged.

### Gates on `c0f8bde` — seven of eight green

| gate | exit | evidence |
| --- | --- | --- |
| `fmt --all --check` | 0 | no diffs |
| `test -p iridium-editor` (no-default) | 0 | 1014 + 3 + 16 passed |
| `test -p iridium-editor --features syntax` | 0 | 1109 + 3 + 16 passed |
| `check -p iridium-bindings --features web --target wasm32` | 0 | clean |
| `clippy -p iridium-editor` (no-default) | 0 | zero diagnostics |
| `clippy -p iridium-editor --features syntax` | 0 | zero diagnostics |
| `clippy --workspace --all-features --all-targets` | 0 | zero diagnostics |
| **`test --workspace --all-features`** | **0** | **2465 passed, 0 failed, 20 ignored** |

Ran at 23:37 in a window that opened at 1-minute load **9.31**, split `--no-run`
(30 s) then run (30 s). ⭐ **2463 → 2465 is exactly the two tests added** —
`jsx_intrinsic_elements_are_coloured_like_components_are` and
`no_byte_range_carries_two_different_highlights` — so the count corroborates
the change rather than merely failing to contradict it. **All eight gates are
green on `c0f8bde` and `48d7bcd`.**

Plus `check --workspace --all-features --all-targets` exit 0, whose only
warning is the pre-existing `block v0.1.6` future-incompat note.

⭐ **A gate threshold has to be scaled to the run it gates.** These seven were
started at 1-minute loads of 10.1–13.2 — above the <10 bar written for the #71
battery — and that was a deliberate, stated relaxation, not drift. Each
finished in 5–20 seconds against a warm cache. The bar exists to stop a
multi-minute run from being started into a closing trough; applying it
unchanged to a five-second gate would refuse work for no benefit.

The one gate left is the expensive one, and it does **not** get that
relaxation: it rebuilds the all-features test binaries. Load went 26.27 then
28.25 immediately after the workspace clippy, with 5- and 15-minute averages at
17.1 and 14.4 — so the box is genuinely held by another seat, not just my own
decaying burst.

### #73 — closed, and measured rather than reasoned

`rustfmt.toml` declared 21 options stable rustfmt silently ignores — **609**
warning lines per `cargo fmt` run, which is where a real one would have hidden.
What they would have done was measured: the whole workspace formatted by
nightly rustfmt with `unstable_features = true`, diffed against the tree.

- **19 changed nothing.** They restated behaviour already in effect.
- **2 accounted for all of it** — `imports_granularity = "Crate"` and
  `group_imports = "StdExternalCrate"`: **339 hunks, 2,681 lines.** Every one
  of those lines is import organisation; no other option moved a character.

The 19 are deleted; the 2 are preserved in the nightly block with their price
attached. After: `cargo fmt --all --check` exits 0 with **zero** warnings, zero
diffs and zero output, down from 64 KB, and `git status` showed `rustfmt.toml`
as the only modified file — so no source formatting changed.

⭐ Recorded rather than dropped: the 19 were no-ops **against this toolchain
and style edition**. An option restating a default stops being a no-op the
moment the default moves — the case in which deleting them would have mattered.

### #42 — closed. One flag, one owner.

`WebEditor` kept its own `read_only` beside `EditorState::read_only`.
`set_read_only` assigned both; **sixteen** binding-level gates read the copy
while the kernel's own gates read the original.

Checked rather than assumed: the `Editor` is built exactly once
(`wasm.rs:400`), `self.editor` is never reassigned, and every assignment to
`read_only` across both crates shows the only production writers are the two
binding setters — the kernel never writes it outside its own tests. So nothing
had diverged. ⭐ **But it held for those reasons rather than for any enforced
one, and the sixteenth reader had no way to tell which flag was
authoritative.** The field is gone; every reader goes to
`self.editor.state().read_only`, and the compiler enumerates the sites.

Gates: wasm check exit 0 zero warnings (before and after `fmt`), workspace
clippy exit 0, full suite **2465 passed / 0 failed**, diff confined to
`wasm.rs` at +23/−29.

⚠️ **Stated plainly: this change is compile-checked only.** `wasm.rs` is gated
on `all(feature = "web", target_arch = "wasm32")`, runs to **3,160 lines** and
contains **zero** `#[cfg(test)]` blocks, so no test anywhere exercises it. That
is not a property of the change — it is why **#43** exists, and #43 is what
would make this testable.

### #75 — opened while there: clippy never lints the wasm target

The battery only ever runs `cargo check` against wasm32. Running clippy there
for the first time fails with **8 pre-existing errors** in
`render/pipeline.rs` and `render/web.rs` — `future_not_send` ×4 and
`arc_with_non_send_sync` ×4 — which the native gates cannot see at any feature
combination, because they fire only on a single-threaded target. **Same shape
as #60.** Not fixed: different crate, untouched by #42's diff, and choosing
between a fix and a documented `#[allow]` for Send-ness on a target with no
threads is a design call, not hygiene.

### The one action that moves things

**#74 — nothing pins the Rust toolchain.** Found while measuring #73, and #73's
own title had assumed a pin that does not exist: there is no
`rust-toolchain.toml` and no `rust-toolchain` file anywhere. CI takes
`dtolnay/rust-toolchain@stable`, the box takes the rustup default, and nothing
makes those agree — or makes today's CI agree with tomorrow's.

⭐ **Two of the eight gates are verdicts rendered by the toolchain, not by the
code.** `clippy -- -D warnings` gains lints on new stable releases and
`fmt --check` can change its mind across rustfmt revisions, so a PR green today
can go red the morning a new stable lands with no commit in between — and the
failure gets attributed to whatever was in flight. All the clippy-gate and
500-line-bar work assumed a green gate means something durable. It does not yet.

Not done unilaterally: pinning changes the toolchain for everyone who builds
this repo, and it interacts with #73's nightly-rustfmt question. Tom's ruling.

### Two things not to redo

⛔ **Do not relaunch anything as a background task.** The load observer was
killed twice and the gate runner once, the last two simultaneously — a harness
reap, not the box. A third attempt is fighting the harness rather than reading
it. Run gates in the foreground.

⛔ **Do not restart the load sampler.** Its purpose was deriving a debounce
span; that got demoted to belt-only once the multi-window finding landed.
Sampling `sysctl -n vm.loadavg` inline at each tick is sufficient for deciding
whether to run. `load.tsv` and `load2.tsv` are separate series — **never
compute troughs across both**, the sampler was killed between them.

### Blocked on Tom, all with written recommendations

| task | needs | where |
| --- | --- | --- |
| **#58** | three key rulings — edit / delete+create / apply. Suggested Tab, Ctrl+D, ⌘S | `IN-FLIGHT-oil.md` |
| **#64** | ruling to delete the 21 MB legacy bindings tree. Recommend yes | `IN-FLIGHT-legacy-bindings.md` |
| **#70** | six colour choices — now the *whole* of that task | `IN-FLIGHT-unstyled-captures.md` |
| **hook** | whether to install Cally's history-rewrite guard in this repo. Recommend yes, with one gap to close first | below |

### Cally's history-rewrite guard — Tom's call, 2026-08-07

Cally asked permission to switch on a guard that refuses history rewrites in a
shared working tree — the class that destroyed a commit here on the 5th
(recorded at `2d5424c`). It watches the ref move rather than the command, so
`--amend`, `reset --hard`, `rebase`, `branch -f`, `checkout -B` and
`--no-verify` all land on it. Installing it needs two things committed here: a
tracked `.shared-tree` marker at the root, and `core.hooksPath` pointed at a
tracked, **relative** directory.

**I declined to authorise it and referred it up.** It changes how git behaves
for Tom and refuses rewrites he may want; the friction is his, not mine.

**Recommend yes.** This seat is already banned from `reset`, `restore`,
`checkout --`, `clean`, `stash` and `worktree` here, so the guard codifies an
existing constraint rather than adding one, and the rebase cost is near zero
because the work commits forward — `bd582e1` was deliberately left saying "four
of six" rather than amended once the other two gates passed.

⭐ **The gap I raised, and the reason it should be closed first.**
`core.hooksPath` is repo-local config. `.git/config` is not cloned. So a fresh
clone gets the tracked marker and the tracked hook scripts and **no
`core.hooksPath`** — git falls back to `.git/hooks/` and the guard is silently
inert, which is the exact failure class Cally cites as motivation. The tracked
marker fixes the *move* case, not the *clone* case, because the thing that
activates the hook is itself a config key.

It is at least **detectable**, which a config-only design would not be:
`.shared-tree` present AND `core.hooksPath` unset-or-wrong is a cheap
assertion. It needs to be something that runs, not a README line.

Two open questions, asked rather than asserted — the hook lives in Cally's tree
and **I have not read it**: whether a `reference-transaction` hook reads a
force-updated remote-tracking ref from `git fetch` as a rewrite, and whether
Tom wants tracked executable hooks running on ordinary git operations in a repo
whose guidelines open with *"mission-critical infrastructure for financial,
legal, and healthcare settings"*.

Verified: no `.shared-tree`, no `core.hooksPath`, no active hooks in this
checkout. Nothing has moved. Waffles' reported 17/17 is taken as reported and
verified by nobody here.

#### Update — the clone hole was reproduced and closed, and it moved

Cally cloned a correctly-armed repo and got `commit --amend` through with no
complaint: marker present, hook file present and executable, `core.hooksPath`
unset. The closure is `tools/hooks/check_guard_active.sh`, **deliberately not a
hook** — a hook that checks whether hooks are armed is inert in exactly the
case it exists to detect. Fixture 11 → 20 arms; arm 17 pins the clone defect,
arm 18 requires the detector to fire on arm 17's state.

She also inverted my supply-chain question, correctly: git's refusal to
auto-arm a hook received from a remote *is* the protection, so manual arming is
the design rather than an oversight, and a loud detector is the only honest
closure.

⭐ **But its home in this repo breaks it, and that is verified here.** The only
committed gate runner in iridium is `.github/workflows/ci.yml` — `actions/
checkout@v4` on `ubuntu-latest`, on push to main and every pull request. There
is **no committed local battery**; the eight-gate battery is a scratchpad
script of mine, uncommitted and invisible to anyone else. So "it runs from
something that already runs" resolves here to CI — and a CI runner is a clone
with the marker tracked and `core.hooksPath` unset, which is the detector's
exit-1 condition exactly. **It would fail on every push and every PR, forever.**

The deeper point: a CI runner never originates a commit, so it has nothing to
protect and its unarmed state is *correct*. The believed-protection alarm would
be firing on a machine that is correctly unprotected, making it
indistinguishable from a real hit — and a permanently-red gate gets muted
within a week, which removes the signal on the developer machines where it is
real. The test it needs is not "is the guard armed" but **"does anyone commit
from this clone"**; `CI` / `GITHUB_ACTIONS` are the available proxies and the
escape should be written down before main goes red, not after.

#### The fetch question — answered, and I was wrong

Measured on git 2.47.1, three ways:

| operation | result |
| --- | --- |
| `git fetch` of a force-updated upstream | **allowed** — writes `refs/remotes/*`, outside the `refs/heads/*` filter |
| `git fetch --prune` | **allowed** |
| `git pull --rebase` onto a rewritten upstream | **REFUSED**, rc 128 |

I raised the fetch case twice and it was unfounded — reasoning from the shape
of the hook rather than from what it does. The real cost is the third line.

⭐ **And it is live in this repo, not hypothetical.** Checked: `origin` is
`github.com/tomWhiting/iridium.git` and `main` tracks `origin/main`. A
catch-up after someone force-pushes upstream would be refused here. Tom should
see that as a real cost. It goes rare-by-construction once force-push is
blocked on the default branch, but that is not in place today.

The CI objection was accepted and fixed by **splitting the assertion** rather
than sniffing the machine: a default `--arming` mode for machines that
originate commits, and an `--artefacts` mode that asserts only that the repo
*ships* an armable guard — marker tracked, hook tracked, committed mode
100755, committed blob carrying the sentinel — and never reads `hooksPath`. CI
runs that one and cannot go red for being a clone. The `$CI`/`$GITHUB_ACTIONS`
proxy I suggested was deliberately refused, correctly: it keys the control on
an incidental property of the machine, which is the original mistake wearing a
new subject.

#### CLOSED: disarming removes the detector along with the guard

Fixed at Cally's `d0c330e`, fixture 26 → 29 arms. Marker present in HEAD but
missing from the worktree is now its own state (rc 3, distinct from rc 1,
because the remedy differs). To silence it legitimately you must
`git rm .shared-tree && git commit` — so the escape stays fully open and simply
stops being silent.

⭐ **The remedy converts a quiet act into a recorded, attributable one rather
than trying to detect it.** That is the better shape: a guard nobody can get
past gets removed wholesale instead of deliberately.

⭐ **The generalisation is worth more than the fix.** *Any control keyed on a
marker is disarmed by deleting the marker. Only controls whose absence is a
detectable state can defend themselves, and that requires a second independent
record of what the marker was.* Here that record is git history and it is free.
Ask that question **before** keying anything else on a file — the usual answer
is that no second record exists, which makes the control decorative in exactly
the case it matters.

**No third objection.** I went looking and did not find one.

#### Install cost in this repo, for when Tom rules

Three things: `.shared-tree`, a tracked `.githooks/`, and a new step in
`.github/workflows/ci.yml` running the **artefacts** mode. The third is a
change to the file gating every PR here, so it wants to land on its own and go
green once before anything depends on it.

#### ⚠️ Two corrections, and one of them is in this repo's history

**1. The `docs <sha>` receipts are unresolvable by class, not by accident.**
`ablative/docs` has no remote, so a digest citing it can never be resolved from
here — verified: `git cat-file -t d0c330e` returns *"Not a valid object name"*,
where the local `2d5424c` resolves fine. Waffles has ruled such citations
local-only-by-class.

The substance travelled inline and stands — arm counts, exit codes, the fetch
table. The digest was the part doing no work. ⭐ **A husk citation is not a lie
about the content, it is a lie about the grammar** — and the grammar is what
tells a reader how hard to lean.

**This one is mine to fix, because I repeated it.** Commit `f952df7` says
*"Closed at Cally's d0c330e"*. That digest is unresolvable in this repo and
always will be. `f952df7` is **not amended** — the correction goes in the tree,
the same remedy `2d5424c` used, because *a correction that lives only in the
channel does not reach the person running `git log`*. Anyone who hits that line
should read the arm counts, not the digest.

**2. The artefacts-check offer is already blocked, and I made it without
checking.** I offered to run `check_guard_active.sh` against iridium's CI once
Tom rules. **The script lives in `docs`, which has no remote — it is
unreachable from this box.** Neither of us spotted it: the citation
conversation was about *references*, and this is about *executables*.

⭐ **A control whose authoritative copy is unreachable is already forked; it
has just not diverged yet.** That is the unmirrored-guard problem from earlier
in this exchange, with the *original* playing the part of the invisible copy.
The install itself is fine — arming a repo lands the hook in that repo, tracked,
and tracked things travel. The **fixture** cannot, so per-repo installs would be
N copies with no reachable source of truth.

Cally has proposed moving the hooks tooling to `tools/gates`, which has a real
remote. That is a location ruling and she has not touched it. **The offer stands
but is not actionable until the script is reachable** — recorded here so it is
not carried as available work.

#### The original open item, now answered above

Raised by me, not yet measured. The sanctioned way to perform a rewrite you
want is to remove the marker deliberately. But `--arming` fires on *marker
present AND guard not armed* — **marker absent is not that state**, so removing
the marker switches off the guard and the thing that would notice, in one move.

⭐ It is demanded at the worst moment. The `pull --rebase` refusal lands when
someone has force-pushed upstream and the seat is mid-incident trying to catch
up. That is exactly when a marker gets removed, the pull finishes, and it never
goes back — with nothing to say so, because the detector is keyed on the file
just deleted.

The signal already exists in the artefact half: the marker is **tracked**, so
*absent from the working tree* and *absent from HEAD* are different states, and
only the second means the repo never had a guard. Marker in HEAD but not in the
working tree is a deliberate disarm in progress and should be loud. That is a
third assertion, not a tweak to the first.

Position unchanged: still recommend switching it on here. The clone hole is
closed and the CI case is a placement problem, not a design problem.

**#72** (`tag.jsx` → `Tag`, `text.jsx` and `nested` → deliberately unstyled)
needs **no ruling** — it was split out of #70 precisely so it would not wait.
It needs only a build.

**#73** — `rustfmt.toml` declares 21 options inert on the pinned stable
toolchain, two of them enforcement options the project believes are active.

### Loop

A ScheduleWakeup is armed for **23:39** with `<<autonomous-loop-dynamic>>`.

### The box, all evening

Another seat's legitimate lane (Vesper's `aion-rs`) held it from roughly 19:45
onward. Four closed troughs measured before the sampler died — **450 / 150 /
601 / 300 s** — and every battery-scale run this evening was declined on that
basis. Full reasoning, including why a multi-window gate never clears here, is
in `docs/IN-FLIGHT-box-gate.md`.

---

# ▶▶ PICK UP HERE — 8 Aug 2026, ~02:10

**This supersedes the "PICK UP HERE — 22:44, 2026-08-07" section above.**

## The eight-gate battery is GREEN, and it found something

Commits: `f08c888` (the fix + the full battery), `aea724d` (docs).

| gate | exit | evidence |
| --- | --- | --- |
| `test --workspace --all-features --no-fail-fast` | 0 | **2472 passed, 0 failed**, 20 ignored |
| `test -p iridium-editor --no-default-features` | 0 | 1033 passed, 0 failed |
| `test -p iridium-editor --no-default-features --features syntax` | 0 | 1128 passed, 0 failed |
| `check -p iridium-bindings --features web --target wasm32` | 0 | zero warnings |
| `clippy --workspace --all-features --all-targets -D warnings` | 0 | **red first — see below** |
| `clippy -p iridium-editor --no-default-features --all-targets` | 0 | |
| `clippy -p iridium-editor --no-default-features --features syntax` | 0 | |
| `fmt --all --check` | 0 | |

Each ran unpiped with `$?` read on the very next line. **2472 is +7 on the 2465
at `322a14a`** — exactly the seven cache tests #76 added. The count is a
cross-check, not a decoration.

⭐ **Gate 5 was red.** `daf1d56` landed #76 with the battery unrun and said so
in its own message; running it produced

    error: this could be a `const fn`
      --> crates/iridium-bindings/src/web_highlight_cache.rs:154:5

One character. The part worth keeping is *which* gate caught it: it fires on
the `lib test` target, which `web_highlight_cache` reaches only because #76
gave it the `any(target_arch = "wasm32", test)` gate. The wasm gate is a
`check`, not clippy; the clippy gates are native. **The module is linted for
the same reason it is tested, and both follow from the same decision.**

Not amended into `daf1d56`. A commit that admits a gate is unrun is not the
same as one that passes it, and the record should show the gate found
something.

## #76 is CLOSED

Four highlight fields → one `WebHighlightCache`, seven real tests, and
`WebEditor` is down from sixteen fields to thirteen. `docs/IN-FLIGHT-76-handoff.md`
has been **deleted** — it described an uncommitted, non-compiling tree that no
longer exists, and leaving it would have been a husk. Everything durable in it
is in `daf1d56`'s message or in the #43 map.

## ⚠️ #43's fold blocker had the WRONG CAUSE — corrected at `aea724d`

`docs/IN-FLIGHT-web-document.md` said the web face duplicates folds because the
kernel's folds go stale after every edit (the plan's finding 3). **Both halves
are false**, and the map inherited the claim from a third document without
checking it.

- `apply_command_internal` calls `refresh_syntax()` on every content change —
  `editor/core.rs:1037`, with a comment saying so. Finding 3 is fixed.
- ⭐ The real cause: **`WebEditor` never tells its `Editor` what language it
  holds.** `set_language` appears in `editor.rs` and `web_folds.rs` and
  **nowhere in `wasm.rs`**; `create_web_editor` builds a bare `Editor::new(...)`
  (`wasm.rs:381`) and the comment there says *"The web surface has no way to
  declare a language yet."* `SyntaxState::sync` opens with `self.tree.as_mut()?`
  (`editor/ast/state.rs:227`), so with no language it returns `None`,
  `refresh_syntax` returns `false`, and the kernel's `fold_state` never gets
  regions.

**The web face's second fold state is not a duplicate of a working one — it is
the only one that works in the browser.**

So the precondition for "read the kernel's folds like the desktop face does" is
not a kernel fix behind a plan step; it is one `set_language` call. But *what*
the browser passes is an **L-0 question** — live and unruled in
`docs/IN-FLIGHT-languages.md` — and `wasm.rs` hard-codes `Language::C` as a
stand-in for "fold on braces" in two places (`:389`, `:398`).

**Recommendation unchanged, reason corrected:** do not carry the folds into
`WebDocument`, and do not let #43 be the commit that decides how the browser
names a language.

Checked and found NOT to be a defect, recorded so nobody spends a window on it
twice: I expected the kernel and web refreshes to brace-scan the same document
twice per keystroke. They do not — the kernel's path exits at that `None`
before it scans. One scan.

⭐ **The rule, twice over now:** a citation chain is not evidence. Both this and
the stale-plan note at the top of this file are the same failure — a document
repeating a claim from another document. Only the code is evidence.

## The rule this session keeps re-earning

**A gate that has not run is not a gate that passed**, and the honest move when
the box is loaded is to say which ones are outstanding rather than to round up.
`daf1d56` did that, and the outstanding one is the one that was red.

## Still blocked on Tom — unchanged, five items

**#58** (Oil, three key rulings) · **#64** (delete the 21 MB legacy bindings
tree — recommend yes) · **#70** (six colour choices) · **#74** (pin the Rust
toolchain — recommend a `rust-toolchain.toml`) · **Cally's hook** (install —
recommend yes). Plus **L-0..L-8** in `IN-FLIGHT-languages.md`, which #43 now
depends on.

## Unblocked backlog

**#75** (clippy never lints the wasm target — 8 errors hiding in
`render/pipeline.rs` and `render/web.rs`) is the strongest next pick: it is the
same shape as the gap gate 5 just caught, one layer out, and it needs no ruling
from anyone. Then #44, #45, #69, #31, #35.

**#43 is now blocked on L-0**, not on a kernel fix. That is a change from what
the strip said.

---

# ▶▶ #75 CLOSED — 8 Aug, ~03:30

**The wasm clippy gate is armed and green.** `dc6a67f`. This supersedes the
"#75 is the strongest next pick" line in the section above.

```
cargo clippy -p iridium-bindings --no-default-features --features web \
    --target wasm32-unknown-unknown -- -D warnings
→ exit 0
```

Count went **68 → 60 → 20 → 14 → 0** across `c1bf4b0`, `97725aa`, `729286c`,
`94953ef`, with the gate landing last at `dc6a67f` — the order
`IN-FLIGHT-wasm-clippy.md` argued for and `ci.yml`'s own comment argues for
generally. Full retrospective at the end of that file; the four things worth
carrying:

1. ⭐ **`--fix` was zero, then 28 — the order is the trick.** It applied all 40
   "machine-applicable" suggestions, failed to recompile because 12 made
   `#[wasm_bindgen]` methods `const`, and rolled the whole file back. Suppress
   the unsound ones *first* and the same command does 60% of the task.
2. ⭐ **`#[expect]` does not survive `#[wasm_bindgen]`.** It silences the lint
   *and* reports `unfulfilled_lint_expectations` — twelve warnings become
   twelve different warnings. Those sites are `allow`, with the lost strictness
   written down.
3. ⭐ **The suppression boundary must match the reason's boundary.** One
   attribute on the `impl` block would have been wrong: twenty private helpers
   live in it and on those the lint is right — `bump` took its advice correctly
   in `f08c888`, one commit earlier the same night.
4. ⭐ **A lint that is vacuous today is a claim about your build configuration,
   not your code.** 13 casts could not truncate on `wasm32` (`usize` is 32 bits
   there). Obeying cheaply via `try_from` beat proving vacuity via `allow`,
   because the vacuity was a fact about the *target* and `wasm64` exists.

**Only the GPU-free kernel configuration is now ungated.** Still a burn-down,
not a flag.

## ⚠️ THE BOX FILLED UP MID-TICK — resolved, but it will recur

`/System/Volumes/Data` hit **623 MB free of 926 GB**. Writes failed for
everything including the harness's own task-output files. Not solely mine —
my whole lane was 25 GB of the 897 GB used — but this session's builds
contributed ~5 GB.

**Freed by deleting `target/debug/incremental` only** (12.6 GB), which took it
to **29 GB free**. Deliberately surgical:

| | size |
| --- | --- |
| `target/` | 23.5 GB |
| `target/debug/incremental` | **12.6 GB — deleted** |
| `target/debug/deps` | 8.8 GB — kept |
| `target/release` | 0.98 GB — kept |
| `target/wasm32-unknown-unknown` | 0.83 GB — kept |

⭐ **`incremental` is the right thing to cut and `deps` is not.** Incremental
state only speeds up rebuilds *after an edit*; `deps` holds compiled
dependencies and costs a full rebuild to recreate. Cutting the larger, cheaper
half fixed the emergency and kept every compiled dependency.

Nothing outside this lane was touched. **Expect this back** — a full workspace
test run regenerates several GB of incremental data, so it is worth checking
`df` before a battery rather than after a failure.

## Gates on the current tree

Ran green: `test --workspace --all-features` (**2475 passed, 0 failed** — +3 on
2472, the new `js_index` tests), `clippy --workspace --all-features
--all-targets -D warnings`, the wasm clippy gate, the wasm `check` (0 warnings),
`fmt --all --check`.

**Not re-run, and verified not to need it rather than assumed:** gates 2/3/6/7
are all `-p iridium-editor --no-default-features`, and `render/mod.rs` gates
`mod pipeline` and `mod web` behind `#[cfg(feature = "render")]`. Nothing they
compile has changed since their green at `f08c888`, and they never compile
`iridium-bindings`.

## Backlog after this

**#43 blocked on L-0** (see the 8 Aug section above). Unblocked and unclaimed:
**#44**, **#45**, **#69**, **#31**, **#35**. Still Tom's: #58, #64, #70, #74,
Cally's hook, and L-0..L-8.

---

# 📮 UNDELIVERED TO TOM — Meridian was down, 8 Aug ~03:45

`mcp__meridian-remote__send` failed **twice** with

```
500: {"error":"proxy resolver: lookup failed: connection error:
     database connection pool acquire timed out"}
```

Under the delivery rule — *anything meant for Tom leaves through Meridian or it
did not happen* — **this did not happen.** It is recorded here so it survives
and gets re-sent, because the first item in it is a **correction to something I
already told him that was wrong**, which is the worst kind of message to lose.

**Retry this on the next tick.** DM `dm:c9255b2a-5731-4d17-8124-e3bfa2224186`.

## ⚠️ ATTEMPT 6 ALSO FAILED — 8 Aug ~04:20

Six attempts, six identical 500s, over several hours. The text sent on attempt
6 is current (it carries both corrections and the palette fix) — **re-send that
one**, it needs no further amending except to add #64's finding.

⚠️ **He has NOT been notified out-of-band.** I tried `PushNotification` on the
reasoning that the delivery channel itself is what is broken, and it declined:
the terminal is active, so this session's own output already reaches him and a
notification would have been a duplicate. Corrected here rather than left
saying he was told, because "he knows" is exactly the kind of thing a later
tick would build on.

So the state is: **the transcript is the live channel, Meridian is not.**

## ⚠️ ATTEMPT 5 ALSO FAILED — 8 Aug, after the fallback-palette fix

Five attempts, five identical 500s. **Stop retrying within a turn** — one
attempt per tick is the right cadence for a channel that has been down for
hours; more just burns the turn.

⚠️ **The queued text below is now two updates behind what should go.** Before
the next attempt, add: (a) the **second correction** — I told Tom #31 was
unblocked and it is not, so with #35 done the unblocked column is *empty*;
(b) the **fallback-palette fix** (`9bb09df`) and the stale claim it found in
`LIGHT-THEME-MAP.md` fact 1, which he should know before ruling D-1.

## ⚠️ ATTEMPT 4 ALSO FAILED — 8 Aug, after #35 landed

Same error again. **Four attempts, four identical 500s.** Meridian has been
unreachable for hours, not minutes.

The queued text now also carries **#35's closure** (see below) — amend before
sending, do not send a stale draft. The running rule stands: the channel being
down never gates the work; the message waits and the backlog moves.

## ⚠️ ATTEMPT 3 ALSO FAILED — 8 Aug, post-compaction

Same error, byte for byte. Three attempts, three
`database connection pool acquire timed out`. That is no longer a blip; treat
Meridian as **down**, not slow.

Two consequences, both already applied:

1. **It does not block the work.** Retrying the send was step 1 of the baton and
   #35 was step 2, but a channel that is down cannot be waited out — so #35
   started anyway and the send stays queued here. A queued message costs
   nothing; an idle seat costs the whole tick.
2. **The text below was amended before attempt 3** and this is the version to
   re-send: it now carries the #75 closure, which happened after the message was
   first written. Send *this*, not the earlier draft.

## The message

> **Correction to what I told him an hour earlier.** I listed **#44 and #45 as
> "unblocked and unclaimed"**. They are not.
>
> #44's wire shape is already finished — `bindings/src/workspace.rs`, 540 lines
> with 470 lines of host tests, and `grep -c workspace` is 0 in both faces.
> Complete, tested, consumed by nothing. So #44 is only the `#[wasm_bindgen]`
> adapters.
>
> But the adapter is what makes two documents possible, and the web face cannot
> have two documents correctly. `DesktopDocument` carries no fold fields — the
> desktop reads `editor.fold_state()`, which is per-document and works only
> because the desktop calls `set_language`. The web face does not, so its fold
> state is one app-wide copy. Ship #44 over that and **tab 2 renders tab 1's
> folds**.
>
> Order is **L-0 → folds → #43 → #44 → #45** — four items behind one ruling,
> not one. The whole web-face track is stalled on it.
>
> No cheap shortcut: `set_language(Language::C)` would fix folds and also drive
> highlighting and comment toggling, so `Ctrl+/` would write `//` in a JSON
> file. That is the L-0 question, not a way round it.
>
> **#69** — not reproduced. 148 runs this session across four conditions, 160
> including the earlier sitting. But load never exceeded 5.6 and the report
> says "under box load"; this box hit 36 and 50 tonight. Not claiming it fixed.
> Two mechanisms eliminated by reading (readback sync; cross-test GPU state).
> What survives: warm and cold compositors hold different glyph-atlas state and
> the test asserts byte-identical pixels. Matches the recorded failure exactly —
> 1 pixel, 1 channel, magnitude 23. Next occurrence is decidable from the
> diagnostic. Test left alone; a flake you silence is a check you deleted.
>
> **#75 closed** since that message: wasm clippy gate armed in CI, 68
> diagnostics to 0. The backlog item said 8 — `-D warnings` reports the first
> *unit* that fails, and `iridium-editor` is a dependency there, so it aborted
> early and the 60 in `wasm.rs` were never reached.
>
> Genuinely unblocked and left: **#35** (HiDPI font scale on display change) and
> **#31** (theme switch, which has his taste in it). Starting #35 now.
>
> **#35 closed** — and not where the title suggested. The **desktop face was
> already correct** (`ScaleFactorChanged` → `rescaled`, which re-does exactly
> the four calls startup derives from the scale factor). The bug is the web
> face, with a kernel defect behind it: `FrameCompositor::cached_char_width`
> was written only by `load_font`, so `set_font_size` gave a right line height
> beside the previous size's character width — the number that turns a column
> into an x and back. Proven red first at 8.428711 against an honest
> 16.857422, exactly the 2x ratio. ⭐ The existing test could not catch it:
> warm and cold *both* load at 14 then set 16, so both carry the same stale
> width and a pixel-identity oracle is blind to it. Web half: `setPixelRatio`
> export plus a re-arming `matchMedia` watcher, because dragging between
> displays usually fires no resize event at all. Nine gates green, `deno check`
> clean, bundle rebuilt and the export verified in it. Not verified: an actual
> two-monitor drag — this box cannot stage it.
>
> Still waiting on him: **L-0..L-8** (blocks four items), #58, #64, #70, #74,
> and Cally's hook.

⭐ **The rule this is an instance of:** a delivery channel that is down looks
exactly like a message that was never worth sending. The only difference is
whether the sender wrote it down.

---

# ▶▶▶ PICK UP HERE — 8 Aug ~03:50, written under a compaction warning

Everything below is state that would otherwise be lost. Supersedes every
earlier "PICK UP HERE" in this file.

## Do these two things first, in this order

**1. RETRY THE MESSAGE TO TOM.** Meridian returned `500 ... database connection
pool acquire timed out` twice. The full text and the DM id are in the
**📮 UNDELIVERED TO TOM** section above. It leads with a **correction** — I told
him #44 and #45 were unblocked and they are not — so it matters more than a
status note. Under the delivery rule it has not happened until it sends.

**2. ~~Then start #35~~ — DONE, closed at `cf7420c`.**
Write-up in `docs/IN-FLIGHT-35-hidpi.md`. Two commits: `e2d47c9` (the kernel
char-width defect, red test first) and `cf7420c` (the web re-apply path).

**3. ⚠️ CORRECTION — the unblocked column is EMPTY, not "#31".**
I wrote "#31 unblocked but carries Tom's taste" a few lines above. That was
wrong and this supersedes it. `docs/design/LIGHT-THEME-MAP.md` already carries
the whole design — three finished candidate variants in `theme/classic.rs`
(platinum / paper / monochrome), transcribed field by field, with a screenshot
harness — and its §5 lists **D-1..D-8 awaiting a ruling**. #31 is blocked the
same way L-0..L-8 blocks the web track. Its *sequencing* blocker ("behind the
context menu") has cleared, since #28 is closed; the rulings have not.

**4. One ruling-free slice was carved out of it and landed** — `9bb09df`,
write-up in `docs/IN-FLIGHT-31-fallback-palette.md`. `set_theme` chose the
fallback keyword palette on `is_dark` alone and never read `theme.syntax`, so
every theme but the two built-ins was ignored on any document the bridge
paints. Wrong in dark today, hence no ruling in it. The two `set_*_theme`
setters that were the mechanism are removed rather than left loaded.

**5. So the next tick has no unblocked backlog item.** Do not manufacture one.
The honest options:

- **#69** — only actionable at the next occurrence; the diagnostic already
  tells the next reader which branch it is. Nothing to do until it fires.
- **More ruling-free slices**, the way #31's was found: read a blocked item's
  design map for a *correctness* claim that holds independently of the
  decision. ⭐ That is the pattern worth repeating — a blocked item is blocked
  on its **choices**, not necessarily on its **defects**.
- **Stop and say so.** Three ticks of nothing means stop, not narrate.

**6. Second ruling-free slice landed, and it was investigation not code** —
`679e592`, in `docs/IN-FLIGHT-legacy-bindings.md`. #64's second question
("what produces the live 2 MB bundle?") is **answered**: the same builder, the
same run, one gzipped. Proved by gunzipping the live `"rust"` entry and
matching its SHA-256 to the dead one's, byte for byte. The real defect is that
`build.ts` has **lost** its gzip step and its gunzip loader template, so the
live bundle is unreproducible — regenerate today and the browser payload
silently goes 2.1 MB → 21.9 MB on a green build.

⚠️ **Ordering, which is the opposite of the obvious one:** restore gzip →
repoint `OUTPUT_DIR` → delete the tree. Deleting first breaks the builder;
repointing first ships the tenfold payload.

⚠️ **A NEW, SMALL GREEN LIGHT IS NEEDED** and it is not a ruling: restoring
gzip cannot be verified without running the builder, which clones **nineteen
grammar repositories** over the network onto a shared box. That is a resource
call, not a design one. It is the only thing between here and a finished fix.

**6b. Third slice: #58 step 4b's logic landed** — `28b49a4`,
`file_tree/mode.rs` with 19 tests. Browse → Edit → Confirm, buffer held inside
the variants that have one, and the rule that a dirty buffer is never dropped
without being asked. **No key is named anywhere in it**, so the three rulings
cost one table row each.

⚠️ `mod mode;` carries a scoped, self-removing `#[allow(dead_code)]` — it has
no caller until `keys` can be written. That is the honest cost of splitting a
ruling-blocked item, and it is recorded rather than hidden. **If a later tick
sees that allow, the fix is to wire `keys`, not to widen the allow.**

⚠️ **The three questions are now the highest-value thing Tom can answer**, and
he was at the terminal at 04:20 — they were put to him again in the transcript
rather than only in Meridian, since Meridian is down:
1. What enters edit mode? (`Tab` is unbound here; no letter can work, every
   plain character goes to the filter.)
2. How is a row marked deleted, and how is one created? (`Ctrl+D` to strike
   through; `Ctrl+N` is taken by *move down*, so "new row" needs another key —
   this is the one where I have no good suggestion.)
3. What applies? (`⌘S` reads as "save this buffer".)

**6c. NEXT STEP, IDENTIFIED BUT NOT STARTED — no edits were made.**

A fourth ruling-free slice, this one inside **#74**. The pin itself is Tom's
(it changes the toolchain for everyone who builds the repo), but the *problem*
#74 names has a half that needs no ruling:

> "Two of the eight gates are verdicts rendered by the toolchain, not by the
> code… a PR green today can go red the morning a new stable lands with no
> commit in between — and the failure gets attributed to whatever was in
> flight."

⭐ **Nothing in CI records which toolchain rendered the verdict.** No job prints
`rustc --version`. So the unattributable failure the doc predicts arrives with
no evidence in the run.

**The change:** add a version-echo step to each of the four jobs in
`.github/workflows/ci.yml` (`check`, `test`, `clippy`, `fmt`) — `rustc
--version`, `cargo --version`, and `cargo clippy --version` / `cargo fmt
--version` in the jobs that use them. It pins nothing and changes no verdict;
it makes the verdict attributable.

⭐ **Why it is coherent rather than busywork:** the clippy job's own comment
already insists a bare number in a durable artifact outlives the command that
made it, and states its counts with the invocations that produced them. The
toolchain is the same class of thing — the one input to those two gates that is
recorded nowhere. Applying the file's own stated discipline to it.

Verified before stopping: `rustc 1.97.1 (8bab26f4f 2026-07-14)`, `cargo 1.97.1`
on this box; **no `rust-toolchain.toml` or `rust-toolchain` file exists**;
workspace `Cargo.toml` declares `edition = "2024"`, `rust-version = "1.85"`.

**7. The pattern, now used three times.** ⭐ *A blocked item is blocked on its
choices, not necessarily on its defects.* #31 gave a code fix (`9bb09df`);
#64 gave an answered question (`679e592`). Both came from reading a blocked
item's own design map for claims that hold whichever way the decision goes.
That is where the remaining ruling-free work is — and note the second one was
worth more than a code change, because it shrank the risk attached to Tom's
ruling rather than working around it.

## The strip, corrected

| | state |
| --- | --- |
| #75 | ✅ closed — 68 → 0, wasm clippy gate armed in CI at `dc6a67f` |
| #76, #42, #71, #72, #73 | ✅ closed earlier this session |
| #43, #44, #45 | ⛔ **all three behind L-0**, verified `d6ff49e` |
| #69 | 🔍 not reproduced in 160 runs; two mechanisms eliminated, one named |
| #35 | ✅ closed at `cf7420c` — see `docs/IN-FLIGHT-35-hidpi.md` |
| #31 | ▶ unblocked, but carries Tom's UI taste |
| #58, #64, #70, #74, Cally's hook, L-0..L-8 | ⏸ Tom's rulings |

## Loop

`ScheduleWakeup` armed for **03:38** with `<<autonomous-loop-dynamic>>`. If that
tick already fired and this is being read after it, re-arm.

## Box, at this moment

Load ~5. Disk **34 GB free** — recovered from **623 MB** by deleting
`target/debug/incremental` only (12.6 GB). ⚠️ **It will fill again**; a full
workspace test run regenerates several GB of incremental data. **Check `df`
before a battery, not after it fails.** `target/debug/deps` was deliberately
kept — incremental is the cheap half to lose, deps costs a full rebuild.

## Gate state on `9c4b23a`

Green on this tree: workspace tests (**2475 passed, 0 failed**), workspace
clippy `-D warnings`, the new wasm clippy gate `-D warnings`, wasm `check`
(0 warnings), `fmt --check`.

Gates 2/3/6/7 not re-run since `f08c888`, and **verified not to need it**: they
are all `-p iridium-editor --no-default-features`, and `render/mod.rs` gates
`mod pipeline` / `mod web` behind `#[cfg(feature = "render")]`. Nothing they
compile has changed, and they never compile `iridium-bindings`. All commits
since are docs, `wasm.rs`, `js_index.rs` and `ci.yml`.

## Two rules earned tonight, both cheap to forget

⭐ **A `-D` gate on a crate with dependencies reports the first *unit* that
fails, not the work.** #75 was filed as "8 errors"; it was 68. The 8 were the
prefix visible before the build aborted in the dependency. Measure burn-downs
with warnings left as warnings.

⭐ **`machine-applicable` is clippy's claim about its own suggestion, not a fact
about your code.** `missing_const_for_fn` on a `#[wasm_bindgen]` method emits
code that cannot compile, and one bad fix rolls back the whole `--fix` batch.
Disarm the unsound lints first, *then* `--fix` works.

Both are written up at length in `docs/IN-FLIGHT-wasm-clippy.md`.

---

# ▶▶▶ BATON — 8 Aug ~05:40, written under a compaction warning

Supersedes every earlier "PICK UP HERE" in this file. Everything below is
committed; nothing lives only in the conversation.

## Do these, in order

**1. One Meridian attempt.** ⚠️ **One per tick, not five** — it has failed
**eight** times with an identical `500 ... database connection pool acquire
timed out`, over several hours. The current text is the one sent on attempt 8
and it is up to date. Tom has *not* been told out-of-band: `PushNotification`
declines because the terminal is active, so **the transcript is the live
channel and Meridian is not**.

**2. The CI version-echo** — section 6c above. Identified, priced, **not
started, no edits made**. Four small steps in `.github/workflows/ci.yml`. Needs
no ruling.

**3. Then look for the next ruling-free slice**, the way the last three were
found. ⭐ **A blocked item is blocked on its choices, not necessarily on its
defects.** Read a blocked item's own design map for a claim that holds
whichever way the decision goes.

## The strip

| | state |
| --- | --- |
| #35, #75 | ✅ closed this session |
| #31 | ⛔ D-1..D-8 — but its fallback-palette defect landed (`9bb09df`) |
| #64 | ⛔ Tom's — but its open question is **answered** (`679e592`) |
| #58 | ⛔ three keys — but step 4b's logic landed (`28b49a4`, 19 tests) |
| #43, #44, #45 | ⛔ all three behind L-0 |
| #69 | 🔍 not reproduced in 160 runs; decidable at the next occurrence |
| #70, #74, Cally's hook | ⏸ Tom's |

**The unblocked column is empty.** Nothing here is a manufactured task.

## The highest-value thing Tom can answer

**The three oil keys** (section 6b) — the logic is done and waiting, and
`mod mode;` carries a scoped `#[allow(dead_code)]` until `keys` can call it.
⚠️ **If a later tick sees that allow, the fix is to wire `keys`, not to widen
the allow.**

Second: **a green light to clone nineteen grammar repos**, the only way to
verify #64's gzip restore. That is a resource call, not a ruling.

## Commits this session, in order

`e2d47c9` `cf7420c` `45b3756` `9bb09df` `0dbaeff` `dd292b4` `679e592`
`af7a680` `19ee7e8` `28b49a4` `b35efe1`

## Housekeeping

Working tree clean apart from `?? .claude/skills/`, which was untracked at
session start and is not mine. Load ~6.4, 30 GB free, tree 17,280,872 KB at
last measure. The loop is re-armed with `<<autonomous-loop-dynamic>>`.

---

# Tick of 8 Aug ~06:10 — baton items 1 and 2

**1. Meridian attempt 9 failed.** Identical `500 ... database connection pool
acquire timed out`. **Nine attempts, nine identical 500s**, now spanning most
of a night. The queued text (📮 above) is current and was sent verbatim as
attempt 9 — it needs no further amending except to add `772ecd7` below. Cadence
holds: **one attempt per tick.**

**2. The CI version-echo landed — `772ecd7`.** #74's ruling-free half. All four
jobs in `.github/workflows/ci.yml` now echo their toolchain before doing any
work: `rustc --version --verbose` and `cargo --version` everywhere, plus
`cargo clippy --version` in Clippy and `cargo fmt --version` in Format, which
are the two gates whose verdict is the toolchain's rather than the code's.

Verified rather than assumed: the workflow parses (`ruby -ryaml`), each of the
four `run` bodies is the intended one, and all four commands exit 0 on this box
— `rustc 1.97.1 (8bab26f4f 2026-07-14)`, `cargo 1.97.1`, **`clippy 0.1.97`**,
`rustfmt 1.9.0-stable`. ⭐ Clippy's number is *not* rustc's, and the lint set
follows clippy's — which is why its version is printed separately rather than
inferred from the compiler's.

⚠️ **One stale citation caught and corrected before commit.** My first draft of
the Format job's comment said `rustfmt.toml` "declares options that only
nightly reads". It does not — #73 deleted the 19 no-ops and left the two live
ones **commented out**. The claim that is actually live is narrower and better:
`rustfmt.toml`'s own comment records that the 19 were measured as no-ops
*against this toolchain and style edition*, and a measurement with a toolchain
attached is only re-checkable if the toolchain is recorded. Same lesson as
LIGHT-THEME-MAP fact 1: **a citation chain is not evidence; only the file is.**

No Rust changed, so the gate battery had nothing new to compile, and it was not
run. Stated plainly rather than implied — a gate that has not run is not a gate
that passed.

**#74 itself stays pending.** The pin is still Tom's: it changes the toolchain
for everyone who builds the repo. What has changed is that its *failure mode*
is now diagnosable while it waits.

**3. The fifth ruling-free slice, and the biggest — `63aa7fc`, task #77.**
Write-up in `docs/IN-FLIGHT-span-nesting.md`.

Found by following the one sentence `IN-FLIGHT-span-precedence.md` ends on:
*"It still does not resolve nested ranges of different extents."* That was
recorded as a reason `@nested` must stay unstyled. It was also a live defect in
both faces.

⭐ **A span nested inside another was discarded entirely.** Both resolvers
walked the spans in start order and skipped any beginning inside a claimed
range — and the container starts earlier, so it took every byte. A probe over
fourteen languages found **37 containments** on ordinary lines: template
literals, f-strings, shell interpolation, regex literals, `: Array<string>`.

⭐ **The red test returned `None`, not a wrong colour.** There was no run for
the interpolated `y` at all. That is this defect's shape: an absence, not a
mis-colouring — which is why nothing downstream could have caught it.

⭐ **One relationship, three answers.** Identical ranges resolve in the kernel;
equal starts gave the inner span the win and then dropped the outer *whole*,
tail included; different starts gave the outer everything. Decided by byte
arithmetic rather than by anything anybody chose — the same defect class
`IN-FLIGHT-span-precedence.md` closed one level down.

The fix is `crates/iridium-editor/src/render/runs.rs`: one payload-generic
sweep, innermost wins, 12 unit tests. ⭐ **It lives in the always-compiled half
of `render` for a reason worth keeping**: its second caller is `wasm.rs`, which
has no tests and which nothing executes, so an algorithm written there could
only ever be verified by reading it.

⚠️ **Two defects fixed in the lines being rewritten** — not scope creep, they
are in the slicing this change replaces:
1. **A panic path in the browser.** `snap_down` existed only in the desktop
   face; both web resolvers sliced raw document offsets against fold-collapsed
   content they drift from. Hoisted; all three use it now.
2. **A per-frame clone of the whole span set** in the legacy web fallback,
   there only because the callee sorted in place.

⚠️ **Appearance changes** — and Tom should hear it from me rather than see it.
There is no taste in it: no new colour, no theme field, no capture remapped,
every affected byte moving from a less specific answer to a more specific one
the grammar already gave. But it is visible, and it is in the queued message.

⚠️ `apps/iridium-desktop/src/highlight.rs` was **843 lines, up from 806 — and
already over the 500 bar before this change.**

**3b. So it was split, `6c2b536`**, on the seam named above and not one invented
for a line count: the tests divided along a banner comment already in the file
marking exactly that boundary.

| file | lines |
| --- | --- |
| `highlight/mod.rs` | 84 — prose, declarations, re-exports |
| `highlight/cache.rs` | 115 |
| `highlight/resolve.rs` | 89 |
| `highlight/tests/mod.rs` | 43 |
| `highlight/tests/frame.rs` | 307 |
| `highlight/tests/window.rs` | 271 |

⭐ **Five intra-doc links broke on the move** — items in scope in one file and
not in another. Found by *running* `cargo doc -p iridium-desktop --no-deps
--document-private-items`, not by reading, and fixed with explicit paths. ⚠️
**That run reports 29 remaining warnings, none in this module** — pre-existing,
elsewhere in the desktop crate, and a real (small) piece of work nobody has
picked up. `cargo doc` is **not** one of the nine gates, which is why they
accumulated.

**4. One lead checked and found NOT to be a defect**, recorded so nobody spends
a window on it twice. `IN-FLIGHT-web-document.md`'s **blocker 3** warns that a
`KeyboardHandler` per document would duplicate the keymap stack per tab, so *"a
config reload reaches some tabs and not others"*. The desktop face does have one
`Editor` — and so one handler — per open document. But `Workspace` already
solves it: `workspace/settings.rs:192` replays every face keymap onto every open
tab **and every future one**, and `workspace/face_setup_tests.rs` tests both
directions by name. Not a defect. The sticky-column half of blocker 3 is
likewise correct today, and only becomes a question when one handler is shared.

---

# ▶▶▶ BATON — 8 Aug ~06:55, written under a compaction warning

Supersedes every earlier "PICK UP HERE" and BATON in this file. Everything
below is committed; nothing lives only in the conversation.

## Do these, in order

**1. One Meridian attempt.** ⚠️ **One per tick, not five.** It has now failed
**ten** times with an identical `500 ... database connection pool acquire timed
out`, across most of a night. The current text was sent verbatim as attempt 10
and is up to date except that it does not yet mention `995e513` (the doc-link
sweep). Tom has **not** been told out of band — `PushNotification` declines
while the terminal is active — so **the transcript is the live channel and
Meridian is not.**

**2. The `cargo doc` question, which is now the only thing left of that sweep.**
`995e513` took the workspace from **57 broken doc links to 0**. What remains is
**64 warnings of one kind**: *"public documentation for X links to private item
Y"*. Those links **work** under `--document-private-items`, which is how anyone
reads an internal crate's docs, so stripping them would make the docs worse.

Two decisions sit on top of that and **I did not make either alone**:
- Add `#![allow(rustdoc::private_intra_doc_links)]` per crate root, with the
  reason stated? (My recommendation: yes — it is true, and it is the only way
  the count can ever mean anything.)
- Gate `cargo doc` in CI? ⚠️ **Unlike the version-echo, a doc gate can turn a
  green PR red**, which is why I stopped. If it is armed, the ci.yml discipline
  applies: **burn down first, arm last.** The burn-down is done except for the
  64 above.

**3. Then look for the next ruling-free slice**, the way the last six were
found. ⭐ **A blocked item is blocked on its choices, not necessarily on its
defects.** Read a blocked item's own design map — or the sentence another
document says it left open — for a claim that holds whichever way the ruling
goes. That last route is how `63aa7fc` was found and it was the biggest of the
six.

## ⭐ One rule earned this tick, cheap to forget and expensive to rediscover

**A `//!` module doc cannot link to a name the module merely imports or
re-exports — only to items it defines.** `span_index/mod.rs` links
``[`SpanIndex`]`` two lines above `pub use interval_tree::SpanIndex;` and it
does not resolve. Every one of the 47 unresolved links was this, or a stale name.
Write the item's own path in a module doc.

Two finds inside that sweep worth keeping:
- Markdown ate `[, ]` in `PunctuationBracket`'s doc as a link to `,`.
- Three links named `iridium_editor::Workspace` and
  `iridium_editor::EditorKeyResult`, **neither of which exists at the crate
  root** — the docs named a re-export that is not there.

## Commits this session, in order

`e2d47c9` `cf7420c` `45b3756` `9bb09df` `0dbaeff` `dd292b4` `679e592`
`af7a680` `19ee7e8` `28b49a4` `b35efe1` `f799f69` `772ecd7` `eb47072`
`17dc423` `63aa7fc` `cab4e26` `6c2b536` `ac6ee1c` `995e513`

## The strip

| | state |
| --- | --- |
| #35, #75, #77 | ✅ closed this session |
| #31 | ⛔ D-1..D-8 — but its fallback-palette defect landed (`9bb09df`) |
| #64 | ⛔ Tom's — but its open question is **answered** (`679e592`) |
| #58 | ⛔ three keys — but step 4b's logic landed (`28b49a4`, 19 tests) |
| #43, #44, #45 | ⛔ all three behind L-0 |
| #69 | 🔍 not reproduced in 160 runs; decidable at the next occurrence |
| #70, #74's pin, Cally's hook | ⏸ Tom's |

**The unblocked column is empty again.** Nothing above is a manufactured task.

## The highest-value things Tom can answer

1. **The three oil keys** (section 6b) — logic done, waiting behind a scoped
   `#[allow(dead_code)]`. ⚠️ **If a later tick sees that allow, the fix is to
   wire `keys`, not to widen the allow.**
2. **A green light to clone nineteen grammar repos** — the only way to verify
   #64's gzip restore. A resource call, not a ruling.
3. ⚠️ **He has not yet acknowledged that `63aa7fc` changes how code looks.**
   Interpolations, f-strings and generic type arguments are now differentiated.
   I judged it taste-free (no new colour, no theme field, no capture remapped)
   and said so in the transcript, but he has not replied. It reverts cleanly.

## Gate state

Nine gates green on `63aa7fc` (2,513 tests, 0 failed). Since then:
`6c2b536` re-ran gates 1/5/9 green; `995e513` is doc comments only and was
verified with `cargo check --workspace --all-features --all-targets` (exit 0)
and `cargo fmt --all --check`. ⚠️ **Gates 2/3/4/6/7/8 have not run since
`63aa7fc`** — and nothing since touches what they compile (docs, the desktop
crate, and comments), which is the reason, not an excuse.

## Box

Load ~9. Disk **170 GB free** — it was 29 GB at the start of this tick, so
something outside this seat freed ~140 GB. Tree 18.7 GB at last `du -sk`.
Working tree clean apart from `?? .claude/skills/`, untracked at session start
and not mine.

---

# ▶ TICK — 8 Aug ~07:15

## 1. Meridian: attempt 11 **SENT**

⭐ After **ten** consecutive `500 … database connection pool acquire timed
out`, the bridge came back. Tom now has, in one message: the appearance
change in `63aa7fc` and why I judged it taste-free; the two defects fixed in
the same lines; the `cargo doc` decision pair (allow per crate root, and
whether to gate it in CI — with the warning that unlike the version-echo a
doc gate can turn a green PR red); the closed list; and the full blocked
column. It also says outright that it is attempt 11 and why he may be reading
it cold.

**Cadence stays one attempt per tick.** The channel being live once is not
evidence it is live twice.

## 2. #78 — a file with no grammar wore another language's keywords

Full write-up: `docs/IN-FLIGHT-grammarless-bridge.md`. Landed `71da0da`.

**How it was found**, because the route is the reusable part: `has_grammar`
is `pub use`d from `iridium-syntax`'s crate root and **called by nothing
outside the crate**. Its own doc says it is *"the question a caller usually
wants answered before deciding a syntax-driven feature is available."* ⭐ **An
exported answer nobody asks is either dead API or a missing wire-up** — and
the second is a defect wearing the costume of the first.

**The chain, all read at named lines, none inferred:**

1. `windowed.rs:162` set `language_active = language().is_some()`.
2. `Highlighter::try_new` answers `None` for a grammarless language, so the
   cache takes its *"the query did not compile"* path — entry empty,
   **`language_active` still true**.
3. `shaping.rs:234` and `:280` read that pair and run the keyword bridge:
   `SimpleHighlighter`, whose `is_keyword` is the **union of the Rust,
   JavaScript, TypeScript and Python keyword sets**.

⚠️ **The worst case is the file `git commit` opens.** A commit message is
prose, and that union contains `for`, `in`, `as`, `if`, `else`, `match`,
`type`, `new`, `from`, `try`, `with`, `where`, `case`, `return`, `use`.

⭐ **The proxy and its divergence.** `language().is_some()` proxies for *"spans
are owed but have not arrived this frame"*. It agrees with its target for
every language that has a grammar — including the case the path was written
for, a grammar whose `highlights.scm` fails to compile. It diverges for the
four `R-1` admitted with no grammar: `diff`, `gitcommit`, `gomod`, `gowork`.
Nothing is owed to them ever, so the bridge is not a bridge, it is permanent.

⭐ **Why it needed no ruling.** The rule was already written **twice by the
code breaking it** — `HighlightSource::language_active`'s *"a file without a
grammar must never wear another language's keyword colors"*, and
`refresh_windowed`'s own restatement. Both sentences say **grammar**; both
were implemented as **language**. `iridium-lang`'s `languages.txt` makes the
same claim from the other end: *"They will not highlight until a grammar is
linked."* They highlighted.

**Red looked like this**, and the detail matters:

```
diff has no grammar: there is nothing to bridge to, and a file without a
grammar must never wear another language's keyword colours
```

⭐ It failed on the **second** assertion — `index().is_none()` already passed.
The spans genuinely never existed, so the fix removes colour that came from
nowhere, not colour that came from somewhere better.

**The fix is one line** plus the three docs that stated the grammar rule while
implementing the language rule. `span_index` is `#[cfg(feature = "syntax")]`,
so no stub counterpart and the parser-free kernel is untouched. Both native
faces inherit it through the one shared cache (parser-tax R2); the terminal
face has no keyword bridge at all, so this makes the two agree.

**Left behind, named not fixed:** the web face answers `language_active` true
unconditionally (`wasm.rs:3079`). That is its host's notion of a set language,
documented on the trait, and changing it means deciding what TypeScript knows
about grammar linkage — L-0 territory.

## 3. Leads checked this tick and found NOT to be defects

Recorded so nobody spends a window on them twice.

- **`SyntaxState` with a grammarless language.** I expected `note_edit`'s
  early return (no tree) to starve the brace-scan fold detector of the deltas
  it needs. It does not: the **stub `SyntaxTree::new` never fails**
  (`syntax_stubs.rs:105-112`, and it says so), so the feature-off build always
  has a tree and always records. Feature-*on* with a grammarless language,
  `refresh_syntax` returns false before folds are touched — folds are absent
  rather than stale, and those four languages have no braces to fold anyway.
- **The light preset's surfaces.** `EditorColors::light()` states `ffffff`,
  `f5f5f5`, `f5f5f5` — all opaque, so the 6e22cbe class of bug is not live.
  ⚠️ Worth knowing: `the_dark_preset_surfaces_are_opaque` has **no light
  counterpart**. That is a missing fence, not a defect, and any #31 variant
  must clear it — `classic.rs` already pins it for the three candidates.

## Gate state

**All nine green on `71da0da`**: 2,514 passed, 0 failed. Every gate was run
unpiped with its exit status recorded on the next line.

## Box

Load ~18 at tick open (high, and not this seat's). Disk 169 GB free.
Working tree clean apart from `?? .claude/skills/`, untracked at session
start and not mine.

---

# ▶ TICK — 8 Aug ~07:35

## 1. Meridian: attempt 12 sent (#78 reported)

The bridge is live two ticks running. Tom has the grammarless-keyword fix in
his own terms — that his commit messages were being speckled in Rust/JS/Python
keyword colour.

## 2. #79 — auto-pairs were six hard-coded characters (`3c31c2fe`, `88dde26a`)

Map: `docs/design/AUTO-PAIR-MAP.md`. S-1 built and landed.

**Route, same as #78**: the manifest reader's own module doc names the plan —
*"the hard-coded tables are replaced one at a time, each against a test
asserting the manifest agrees with what it replaces."* Comment tokens were
done (#66). **Brackets were not**, and ⭐ the test that sentence names is what
makes it a defect: the manifest does **not** agree with what it would replace.

**The divergences**, all subtractions once the data is parsed properly:
Rust declares no `'` (a lifetime is not a character literal, so `fn f<'a>`
became `<'a'>`); Rust and JSON and YAML pair backticks they never accept;
YAML pairs `(`; `go.mod` gets five pairings past the one it declares; Markdown
declares all three quotes `close = false`; `diff` declares an empty table and
got six.

## ⚠️⚠️ 3. THE MISTAKE, recorded because it nearly shipped

The map's first draft claimed **Markdown wants `*` paired** and built a whole
slice (S-2) plus a decision (B-2) on it. **Wrong.** The extraction used a
regular expression, and `\{[^}]*\}` terminates on the `}` inside `end = "}"` —
so every `{`→`}` row was silently dropped **and no row's `close` flag was read
at all**. Markdown declares `*` with `close = false`; it does not want it.

⭐ **The rule: parse the format, do not match it.** `tomllib` took ten seconds
and disagreed with the regex on two of nine claims. The map now carries both
tables, the wrong one labelled, because the correction is the more useful
artefact than a clean-looking document.

This is the same lesson as `rustfmt.toml`/#73 in a new costume: **a citation
chain is not evidence; only the parse is.**

## 4. What landed, and what it deliberately did not

`close` defaults to **true** when the key is absent — seventeen rows rely on
it, and reading it as false would have disabled `{` in every C-like language.
A `close = false` row is a *matching* rule for highlighting and navigation,
not a typing rule.

⭐ **Three silences told apart from one refusal.** No language, no manifest,
and no `brackets` key all mean "has not said" → keep every pair. ⚠️ **`awl` is
in that third group, so Tom's own language is untouched.** An empty table
means "pair nothing" → `diff`.

Skip-over and backspace narrowed by the same set, deliberately: in Rust `'a'`
is a literal the user typed, and stepping over the closing quote or eating
both halves would drop input.

Not done, and named in the map: `<`→`>` (S-2/B-2, recommend **no** — `<` in
Markdown is a literal at least as often as an autolink); multi-character
openers `"""`, `r#"`, `/*` (S-3); `not_in` scope constraints (S-4, recommend
**not yet** — the only slice that puts a syntax query on the typing path);
`autoclose_before` (S-5, recommend before S-3).

## 5. Red proof, and the honest note about its order

⚠️ The tests were written **after** the fix compiled. Red was demonstrated by
restoring the old behaviour verbatim — `AutoPairs::for_document` returning
`Self::ALL`, which is exactly what `pair_close` was — and re-running. **4 of
the 7 new tests went red.** The other 3 pass both ways *by design*: they
assert the pairs each manifest **does** declare still work, so the suite
cannot be satisfied by an implementation that merely stops pairing.

## Gate state

**All nine green on `88dde26a`**: 2,525 passed, 0 failed (2,514 before, +11).
Two clippy findings on the way, both in new test code: `needless_collect`
twice, and `doc_lazy_continuation` where a `///` paragraph followed a list.

## Box

Load 35 at tick open, 25 by mid-tick — high, and not this seat's.
Disk 149 GB free. Working tree clean apart from `?? .claude/skills/`.

## For the next tick

1. One Meridian attempt: report #79, and **ask B-2** (Markdown's `<`) — it is
   the only auto-pair question with taste in it.
2. S-5 (`autoclose_before`) is the next ruling-free slice in this vein and is
   already priced in the map.

---

# ▶ TICK — 8 Aug ~08:15

## 1. Meridian: attempt 13 sent

#79/S-1 reported, **B-2 asked** (Markdown's `<`→`>`, my recommendation: no),
and the regex mistake disclosed to him in full rather than left in a file.
Third successful send in a row.

## 2. #79 S-6 landed (`d013abfc`) — Enter expansion follows `newline`

⭐ **The route this time was my own half-made claim.** S-1's write-up said the
editor's fixed `{ [ (` expansion table *"agrees with the manifests'
`newline = true` rows for every language checked"*. **"For the languages
checked" was doing load-bearing work in a sentence that read like a
clearance.** Checking all of them:

| language | expands per manifest | expanded before |
| --- | --- | --- |
| `gitcommit`, `regex`, `jsdoc`, `diff` | **nothing** | `{` `[` `(` |
| `json`, `jsonc`, `yaml` | `{` `[` | `{` `[` `(` |
| `gomod`, `gowork` | `(` | `{` `[` `(` |
| `bash` | `{` `(` | `{` `[` `(` |

Enter between `(` and `)` in a commit message grew a three-line indented block
out of a parenthesis in prose.

⭐ **Pure subtraction, provably**: every language's `newline` set is a subset
of `{ [ ( <`, and `<` is never paired, so no language gains an expansion.
Same argument as S-1, same ruling (#66).

⚠️ **`close` and `newline` are independent flags** and default opposite ways —
`close` true when absent, `newline` false. Rust declares `<`→`>` as
`close = false, newline = true`: *do not type the closer for me, but do expand
the block if I typed it myself.* The two serde defaults are now documented
against each other so neither gets "tidied" into the other.

`AutoPairs` → `PairRules`, two masks. `bracket_close` deleted. Both manifest
accessors share one filter so the single/multi-character rules cannot drift.
`expands` is intersected with the three brackets — removes nothing today,
but makes a `newline` quote in a vendor refresh a decision, not a surprise.
`BRACKET_MASK` names positions in `PAIRS` by hand, so a test pins that it
selects exactly `(`, `[`, `{`.

## 3. ⭐ The rule this tick earned

**A hedge inside a clearance is a defect waiting.** "For the languages
checked" and "as far as I could tell" read as diligence and function as
cover. If a claim is worth writing in a design map, it is worth checking
exhaustively before it goes in — or worth writing as an open question with
the check named. This is the second self-inflicted finding in two ticks
(the first was the regex extraction).

## 4. What I deliberately did NOT build, and why

**S-5 (`autoclose_before`)** was the plan for this tick and I stopped. The
manifests give the character *sets* but not the *semantics*, and the load-
bearing part — whether end-of-line and whitespace implicitly permit closing —
is nowhere in the data. Every set is punctuation only, so whitespace must be
handled outside the set or the feature could not work at all; that is a strong
inference, but it is an inference, and Zed's implementation is not vendored
here to check against. ⚠️ **Two ticks after shipping a wrong claim from a
convenient inference, this is not the moment to ship another.** It is written
up as **B-5** for Tom instead.

## Gate state

**All nine green on `d013abfc`**: 2,530 passed, 0 failed (2,514 → 2,525 → 2,530
across S-1 and S-6).

## Box

Load 17 at tick open. Disk 131 GB free — down 18 GB in ~40 minutes, and **not
this seat**: the only writes here are source files and cargo's incremental
target dir. Worth a glance if it keeps falling.

## For the next tick

1. One Meridian attempt: S-6, and **B-5** (the `autoclose_before` semantics).
2. The auto-pair vein still has S-3 (multi-character openers) and S-4
   (`not_in`), both priced in the map, both with real design content. Neither
   is ruling-free.
3. ⭐ **The productive route remains: read a document's own hedges.** Three
   findings in three ticks came from a sentence that qualified itself —
   `IN-FLIGHT-span-precedence`'s "still does not resolve nested ranges", the
   manifest reader's "replaced one at a time", and now my own "for the
   languages checked".

---

# ▶▶▶ BATON — 8 Aug ~08:20, written under a compaction warning

Supersedes every earlier BATON in this file. Everything below is committed.

## Do these, in order

**1. One Meridian attempt per tick.** ⚠️ **One, not five.** The bridge failed
ten times overnight (`500 … database connection pool acquire timed out`) and
has since succeeded **three times running** — attempts 11, 12, 13. Do not
treat that as permanent. Next message owes Tom: **S-6** (`d013abfc`) and
**B-5**.

**2. Two questions are with Tom and neither should be answered alone:**
- **B-2** — should Markdown auto-close `<`→`>`? The only single-character
  pairing any manifest declares that we do not offer. **Recommend no.**
- **B-5** — `autoclose_before`'s semantics. The manifests give the character
  sets, not what happens at **end of line** or **before whitespace**, and
  neither appears in any set. Strong inference that both must permit closing
  (every set is punctuation only, so whitespace must be handled outside it);
  **but it is an inference**, Zed is not vendored here, and this map already
  shipped one wrong claim from a convenient inference. Recommend building S-5
  with the assumption stated in code — **after he says so**.

**3. Then the next ruling-free slice.** ⭐ **The route that has now paid three
times in three ticks: read a document's own hedge.**
- `IN-FLIGHT-span-precedence`'s *"still does not resolve nested ranges"* → #77
- the manifest reader's *"the hard-coded tables are replaced one at a time"*
  → #79 S-1
- **my own** *"for the languages checked"* → #79 S-6

## ⭐ Two rules earned this session, both expensive to rediscover

**A. Parse the format, do not match it.** The auto-pair map's first draft
claimed Markdown wanted `*` auto-closed. `\{[^}]*\}` terminates on the `}`
inside `end = "}"`, so every `{` row was dropped **and no `close` flag was
read at all**. `tomllib` disagreed with the regex on two of nine claims. The
map keeps both tables, the wrong one labelled.

**B. A hedge inside a clearance is a defect waiting.** "For the languages
checked" reads as diligence and functions as cover. Check exhaustively before
a claim goes in a design map, or write it as an open question with the check
named.

## Where #79 stands

`docs/design/AUTO-PAIR-MAP.md` is the map. Landed: **S-1** (`88dde26a`, the
pair set is the language's) and **S-6** (`d013abfc`, Enter expansion follows
`newline`). Remaining and priced: **S-2** (Markdown's `<`, needs B-2),
**S-3** (multi-character openers — `"""`, `r#"`, `/*`; a different data
structure and skip-over rule), **S-4** (`not_in` scopes — ⚠️ the only slice
that puts a syntax query on the typing path; recommend **not yet**),
**S-5** (`autoclose_before`, needs B-5).

⚠️ Trap for whoever builds S-3: `Manifest::single_char_pairs` drops
multi-character rows **at the reader**, deliberately. Do not "fix" it by
taking the first `char` of `r#"` — that pairs a bare `r`.

## Commits this session, in order

`e2d47c9` `cf7420c` `45b3756` `9bb09df` `0dbaeff` `dd292b4` `679e592`
`af7a680` `19ee7e8` `28b49a4` `b35efe1` `f799f69` `772ecd7` `eb47072`
`17dc423` `63aa7fc` `cab4e26` `6c2b536` `ac6ee1c` `995e513` `b373a77`
`71da0da` `336c3ed` `3c31c2f` `88dde26a` `8ce55362` `d013abfc` `8e2f6841`
`1f158d97`

## The strip

| | state |
| --- | --- |
| #35, #75, #77, #78 | ✅ closed |
| #79 | 🟡 S-1 + S-6 landed; S-2/S-5 need rulings, S-3/S-4 open |
| #31 | ⛔ D-1..D-8 — `theme/classic.rs` already carries three candidates |
| #64 | ⛔ Tom's — open question answered (`679e592`) |
| #58 | ⛔ three keys — step 4b's logic landed behind a scoped allow |
| #43, #44, #45 | ⛔ all three behind L-0 |
| #69 | 🔍 not reproduced in 160 runs |
| #70, #74's pin, Cally's hook | ⏸ Tom's |

⚠️ **If a later tick sees the `#[allow(dead_code)]` on #58's `mod mode;`, the
fix is to wire `keys`, not to widen the allow.**

## Everything blocked on Tom

L-0..L-8 (blocks #43/#44/#45) · D-1..D-8 (#31) · #58's three keys · #64
(recommend delete) · #70's six colours · #74's pin · Cally's hook · the green
light to clone nineteen grammar repos · **B-2** · **B-5** · the `cargo doc`
pair (allow `private_intra_doc_links` per crate root — recommend yes; gate
`cargo doc` in CI — ⚠️ **can turn a green PR red**, so I stopped) · and an
acknowledgement that `63aa7fc` changes how code looks.

## Gate state

**All nine green on `d013abfc`**: 2,530 passed, 0 failed. `8e2f6841` and
`1f158d97` are documentation only.

## Box

Load ~32 and not this seat's. ⚠️ **Disk 130 GB free, down from 170 GB three
ticks ago** — the only writes from here are source files and cargo's
incremental target dir, so something else is consuming it. Worth a `du` if it
keeps falling. Working tree clean apart from `?? .claude/skills/`, untracked
at session start and not mine.

---

# Tick — 8 Aug ~09:05 — #80, and the route that found it

## Sent to Tom

Meridian attempt **14**, succeeded (four in a row now). Carried **S-6** and
asked **B-5**, and re-stated **B-2**. Both still open.

## #80 — CLOSED (`815bec99`)

Two defects in the search path, one root: **it asked a byte a question only a
character can answer.**

- **A — a panic.** `find_literal_matches`'s case-sensitive branch resumed at
  `match_start + 1`. That byte is inside a multi-byte character, and the next
  `text[start..]` panics. Case-sensitive search for `é` in `ééé` crashed the
  editor.
- **B — an over-match.** `is_word_boundary` read one byte per side and asked
  `is_ascii_alphanumeric`. Every byte of a non-ASCII character is `>= 0x80`,
  so `ï` read as a non-word character: whole-word search called `na` a whole
  word inside `naïve`. `replace.rs` shares the check, so replace-all would
  write into the middle of a word.

Both proven red first. Nine gates green, **2,533 passed**. Full write-up in
`docs/IN-FLIGHT-word-boundaries.md`.

## ⭐ Two more rules, both paid for here

**C. A fix applied to the path you are standing on is not a fix to the
defect.** Twelve lines below the panic, the case-insensitive path *already*
advanced by `char::len_utf8`, with a comment explaining that exact hazard.
Someone met it, understood it, wrote it down, and hardened the branch they
were looking at. When a hazard is found, ask **where else does this shape
appear** — the answer is often three lines away.

**D. A claim of uniqueness is checkable, so check it.** This started from
`motions::is_word_char`'s own sentence: *"the single word-character definition
shared by…"*. There were three. One was a harmless copy; one diverged. ⭐ This
is the **hedge route's twin** — a hedge admits a gap, a uniqueness claim
denies one. Both are load-bearing sentences a document wrote about itself, and
both are cheap to falsify.

## Left on the table, named

- **`wasm.rs:2259 char_class`** — a *fourth* definition of "word character",
  in the web face, for word motion. Character-wise and in agreement today, so
  not a defect; but it is the shape #38 closed for `pixel_to_index`. Worth its
  own item.
- **`search/find.rs` 783 lines, `search/replace.rs` 878** — both over the
  500-line bar, tests inline in each. Not acted on.

## Box

Load ~57, not this seat's. Disk **129 GB free** — 130 last tick, so the
170 → 130 slide has **stopped**; it was not this seat and needs no `du`.

---

# Tick — 8 Aug ~09:35 — the uniqueness-claim sweep

Meridian attempt **15**, succeeded (five in a row). Reported #80. **B-2** and
**B-5** still open; said plainly that neither blocks me.

## The sweep

Rule D from last tick — *a claim of uniqueness is checkable* — turned into a
sweep: `grep` for "the only place", "the single definition", "exactly one",
"nothing else reads/writes/calls". Twenty-five hits. Four checked:

| claim | verdict |
| --- | --- |
| `render/runs.rs:134` — "the top of the stack is the only place a duplicate can hide" | ✅ **sound.** Degenerate spans are retained out at :87 before the sort, which is the only thing that could pop the top and let a duplicate past. Clamping to `len` can *create* duplicates, and the first-wins rule handles them correctly. |
| `editor/core.rs:288` — "the only place folds are recomputed" | ✅ **sound** for `EditorState`. Reached from both mutation paths (`apply_command_internal` :1037, history replay :1142), so the plan's stale-fold finding is closed. The web face's own `web_folds.rs:90` is a separate, documented fold state. |
| `wasm.rs:1164` — "the single choke point every content mutation funnels through … exactly once per mutation" | ❌ **incomplete → #81** |
| `motions::is_word_char` — "the single word-character definition" | ❌ closed last tick as #80 |

## #81 — CLOSED (`edffe6e1`)

`setContent` replaces the whole document and dropped two things derived from
the old one — the pending edit, the parse tree — but not the third, the
worker's highlight spans. `WebHighlightCache` is keyed on its own generation,
never on a document revision, so nothing else noticed. The controller renders
synchronously after `setContent` while the worker request is still in flight,
so **opening a file painted it in the previous file's colours** at the
previous file's offsets. With no syntax worker at all, indefinitely.

⚠️ **No red test, and it is written down rather than skipped quietly.**
`wasm.rs` is `target_arch = "wasm32"` only and its gate is a `check`, so
nothing native can build a `WebEditor`. The crate's own answer is extraction
(`web_span_index`, `web_highlight_cache` are gated `any(wasm32, test)` for
exactly this) — but that structure is **#43**'s, and #43 is blocked. First
thing to test when #43 lands. Write-up:
`docs/IN-FLIGHT-stale-highlights-on-load.md`.

## ⭐ Rule E

**A choke point only funnels what is shaped like the thing it funnels.**
`record_edit` takes an `EditSpan`. Every path that *edits* reaches it, so the
enumeration in its doc is true and reads as exhaustive. The mutation that
replaces the document produces no span, so it cannot arrive — and its absence
looks like coverage. When a doc enumerates the callers of a choke point, ask
what mutation the parameter type cannot express.

## #82 — opened, needs Tom (same species as #64)

`wasm.rs` carries a **second implementation of word motion** — `char_class`,
`find_word_boundary_left/right` and six exports. Nothing calls them: the live
controller declares all six on its interface and invokes none, because word
motion goes through `handleKeyEvent` and the kernel. The only call sites are
in the legacy `iridium-bindings/ts` that #64 is waiting to delete.

Read side by side they agree on every reachable input; the single divergence
(a caret past line end — kernel clamps and moves a word, the copy returns the
line end) is unreachable today. So: not a live defect, a **trap** — a second
definition of a kernel behaviour in the one file no test can reach.
**Deleting public wasm exports is Tom's call, exactly as #64 is.** Recommend
delete.

## Box

Load ~25. Disk **139 GB free**, up from 129 — the slide has reversed, nothing
to chase.

---

# Tick — 8 Aug ~10:05 — the sweep continues, four sound, one wrong

Meridian attempt **16**, succeeded (six in a row). Carried #81, asked #82,
and reported the desktop check that came back clean.

## Claims checked this tick

| claim | verdict |
| --- | --- |
| `commands/binding.rs:15` — "deserialization is the only path that can violate [the non-empty sequence]" | ✅ **sound.** All three constructors go through `build_sequence(first, rest)`, no mutator drains the vec, and `KeyBinding` derives `Serialize` only — `Deserialize` is on `BindingData` behind a `TryFrom` that rejects empty. |
| `input/keyboard/types.rs:235` — AST verbs performed "in exactly one place, reached identically by a keystroke and by a command from the palette" | ✅ **sound.** `handle_key` (`core.rs:561`) and `run_command` (`:629`) both end in `consume_key_result`. |
| `apps/iridium-desktop/src/app/viewport.rs:215` — "`load_font` is the only path that remeasures" | ✅ harmless. Slightly loose (the strip reads the *overlay*'s measurement and `overlay.set_font` is the call that moves it) but `sync_top_inset()` runs after both, so the ordering it exists to protect holds. |
| `iridium-tui/src/frame/geometry.rs:55` | ✅ not a claim about today — a note for TUI mouse hit testing, which does not exist yet. |
| `span_index/windowed.rs:120` — "**That half is unproven**" | ❌ **wrong → `dc72b23c`** |

## The desktop check that came back clean — worth recording

#81 was *a replaced document leaves stale colours behind*, so the obvious
question is whether switching tabs on the desktop does the same. **It does
not.** `HighlightCache` lives on `DesktopDocument`, the workspace's
per-document payload (#41), so every file has its own and it dies when the
last tab onto that file closes. Had it been an app-wide field, the reuse gate
in `WindowedSpanCache::refresh_windowed` — `(language, parses, revision,
covered window)`, with **no document identity in it** — would happily hand
file B the spans derived from file A: two freshly opened files of the same
language present identical tuples.

⭐ Recorded because *the absence of a bug is worth writing down when it is
structural.* The desktop is right for a reason, not by luck, and the reason
is one field's location.

## `dc72b23c` — a comment that told the next reader not to look

`Generation`'s doc claimed the revision half of the cache key was unproven,
citing a probe that found `Document::revision` reporting `0` either side of a
whole-content replacement. The code does the opposite:
`Document::continuing_from` exists precisely so a replacement *continues* the
counter, `set_content` uses it, and
`replacing_the_content_does_not_replay_a_revision_the_old_text_already_used`
already pins it. The probe was measuring a `Document::new`.

Left alone it read as an invitation to delete the guard, and deleting it
would reintroduce the bug `continuing_from` was written to prevent.

⭐ **Rule F: a comment saying a guard is untested is worse than no comment —
it stops the next reader looking for the test that exists.** The correction
also states the narrower truth: within one `Editor` parses and revision move
together so the revision decides nothing, and it becomes load-bearing the
moment one cache meets a *different* `Editor` — which nothing in the type
prevents.

## Box

Load ~49. Disk **121 GB free** (139 last tick, 129 the one before) — it
oscillates with something else on the box, not this seat. No action.

---

# Tick — 8 Aug ~10:35 — Waffles' manifold read (three rounds), #83 closed

The sweep was interrupted by a real request and this tick went to it.

## What happened

**Iridium is now one of the three organs of manifold's face** (the other two:
a tree sidebar and GraphMother's interactive graph). Ruling being priced:
*the editor is a component bound to a resource provider; first provider is
files in the estate's repos, read-first; the editor never grows its own
storage* — plus an attention ladder, dot → pill → card → full window.

Waffles asked for a source-verified read with file:line receipts, objections
welcome. Three rounds. **Everything is in
`docs/MANIFOLD-EDITOR-READ.md`** — do not re-derive it, read it.

## The headline corrections I made

1. **"Highlighting proven live" splits in two.** Native is real tree-sitter in
   Rust; the browser has **no parse tree** — the wasm build compiles with
   `syntax` off and takes spans from a JS worker over a spans-only protocol.
   Accepted in full; he will never say "highlighting proven" again without
   naming which.
2. **`4179` appears nowhere in this tree.** A manifold port worn as an iridium
   fact. Accepted.
3. **`onChange` hands out the whole document while the inbound path is
   incremental.** Adopted as written: the provider seam consumes
   `takeLastEdit`'s span, string as fallback.

## The correction *he* made to *me*, which was the best moment

I priced G1 (no parse tree in the browser) as two bad options. He asked
whether the syntax worker's own tree could serve as an oracle. **It can** —
`packages/@iridium/syntax-worker/src/worker.ts:22` retains a live `Tree`,
incrementally edited. My inherited framing had rejected "reimplement
navigation in TypeScript"; his framing was *worker answers about structure,
Rust keeps the semantics*, which is a different thing and was never
distinguished.

⭐ **He challenged an absence claim instead of accepting it, and that is what
turned it up.** An absence claim carries the search that failed — and the
search that failed is the thing worth challenging.

What I found once I looked: the cost is **not** protocol vocabulary. The
UTF-16↔UTF-8 boundary is already solved and tested (`encoding.ts` +
`encoding.test.ts`), which normally *is* the expensive part. The real work is
a **node abstraction** — `editor/ast/walk.rs:23` and `expand.rs` are already
pure `Node → Range` functions with zero editor coupling, but bound to
`tree_sitter::Node` concretely. One trait, two satisfying trees, one
implementation of semantics. Ruled: G1 reclassified from *decide before build*
to *widen when structural features arrive*.

## What survived my pushback

He proposed G2 (no tabs in the web face) dissolves because the composition
*is* the tab strip. **Agreed for tabs, refused for document identity:**
`workspace/model.rs:177` — two tabs share one buffer, one undo history, one
cursor set. Two mounted components onto one resource would get two of each,
divergent, both writing back. Ruled: **one live editor per resource,
guaranteed by the slot**, with provider-owned convergence pre-ruled for the
record-native provider **and its undo consequence chosen in writing then,
not discovered**.

## #83 — CLOSED (`5792a248`)

The custom element listed `readonly` in `observedAttributes` and documented
it, and read it nowhere. Setting it did nothing. Found while reading for a
different question entirely. Now applied at mount (`hasAttribute` — an element
that arrives with it set never fires the change callback) and on change by
presence, not value.

⚠️ **No red test, for a reason worth carrying:** the TS suite is 104 tests
over six files, all pure state machines, no DOM; the element needs a document
and a live WebGPU editor and neither happy-dom nor jsdom is a dependency. So
**both ends of the web face are outside their own gate battery** — `wasm.rs`
unreachable by the Rust gates, `element/index.ts` by the TypeScript ones.
Ceiling on any claim about that layer is "type-checks and the suite still
passes", and I claimed exactly that and no more. Verified `deno check` clean,
`bun test src/` 104/104.

⚠️ The TS tests run under **bun**, not deno — `deno test` fails on
`bun:test` imports. `deno check` is the type-checker, `bun test src/` is the
suite.

## Left alone deliberately

G4 — `element/index.ts:101` `min-height: 200px` on `:host`, a floor the
ladder's bottom rungs sit below. A component's minimum size is the host's
call; Waffles has ruled it falls to the attention states.

## Still Tom's

**B-2**, **B-5**, **#82**, and everything on the old list. Nothing here
changes those.

---

# ▶▶▶ BATON — 8 Aug ~10:35, written under a compaction warning

Supersedes every earlier BATON in this file. Everything below is committed.

## Do these, in order

**1. One Meridian attempt per tick. ⚠️ One, not five.** Attempts 14–16 all
succeeded; the bridge is healthy but was not overnight. Tom's last message
(this tick) covered the manifold read and #83. He owes **B-2** and **B-5**,
and now **#82** as well.

**2. Read `docs/MANIFOLD-EDITOR-READ.md` before touching the web face.**
Iridium is now one of three organs of manifold's face. Three rounds with
Waffles, all rulings recorded there. Do not re-derive it.

**3. Then the next slice, by the routes below.**

## ⭐ The six rules earned, in the order they cost something

- **A. Parse the format, do not match it.** `\{[^}]*\}` terminates on the `}`
  inside `end = "}"`. `tomllib` disagreed with the regex on two of nine claims.
- **B. A hedge inside a clearance is a defect waiting.** "For the languages
  checked" reads as diligence and functions as cover.
- **C. A fix applied to the path you are standing on is not a fix to the
  defect.** Twelve lines below #80's panic, the sibling branch was already
  hardened *with a comment explaining that exact hazard*. Ask **where else
  does this shape appear** — often three lines away.
- **D. A claim of uniqueness is checkable, so check it.** The hedge route's
  twin: a hedge admits a gap, a uniqueness claim denies one.
- **E. A choke point only funnels what is shaped like the thing it funnels.**
  `record_edit` takes an `EditSpan`; a document *replacement* has none, so it
  never arrives and its absence reads as coverage.
- **F. A comment saying a guard is untested is worse than no comment** — it
  stops the next reader looking for the test that exists.

⭐ **The productive routes, in yield order:** read a document's own hedge ·
check a claim of uniqueness · ask what the choke point's parameter type
cannot express · **and when someone challenges an absence claim, look again**
(that is how the worker's retained tree turned up and reclassified G1).

## This session's commits

`815bec99` #80 · `fdcd3051` tick · `edffe6e1` #81 · `dc72b23c` revision
comment · `bb73bd3a` manifold read · `5792a248` #83 · plus tick records.
Earlier: `88dde26a` #79 S-1 · `d013abfc` #79 S-6.

## Gate state

**All nine green.** Rust: **2,533 passed, 0 failed** (last full run before
`bb73bd3a`; everything after is documentation or TypeScript).
TypeScript: **104 passed, 0 failed** — ⚠️ run with `bun test src/` from
`packages/@iridium/core`, **not** `deno test` (the suites import `bun:test`).
Type-check is `deno check src/element/index.ts`.

## The uniqueness sweep — where it stands

**Checked and sound (8):** `runs.rs:134` duplicate-at-top · `core.rs:288`
folds · `binding.rs:15` non-empty sequence · `types.rs:235` AST verb parity ·
`viewport.rs:215` (loose but harmless) · `geometry.rs:55` (not a claim about
today) · **`tree.rs:9` `SyntaxTree` single owner — verified, no `Parser` or
`Tree` field anywhere else in `iridium-syntax`** · `suffix.rs:130`
`entry_claims` byte index (guarded by the length test above it).

**Checked and wrong (2):** `wasm.rs:1164` → #81 · `windowed.rs:120` →
`dc72b23c`.

**Unchecked (~14):** `iridium-tui/src/input.rs:28` and `:183` ·
`frame/line.rs:3` · `workspace/model.rs:282` · `actions/mod.rs:10` ("the only
place an id string is tied to behaviour" — ⚠️ host commands are a second
place ids meet behaviour; worth reading) · `expand.rs:197` ·
`app/mod.rs:12` · `file_tree/panel.rs:346` · `apps/iridium/src/app/mod.rs:292`
· `compositor/mod.rs:1` · `commands.rs:76` · `suffix.rs:21` ·
`iridium-tui/src/frame/search/matches.rs:19`.

## Open work

- **#79** — S-1 and S-6 landed. **S-2** needs B-2, **S-5** needs B-5, **S-3**
  is an *addition* (auto-closing `/*`, `"""`, `r#"`) so it wants a ruling too,
  **S-4** recommend not yet. ⚠️ Trap: `single_char_pairs` drops multi-char
  rows **at the reader**, deliberately — do not take the first `char` of `r#"`.
- **#82** — the web face's uncalled second word-motion implementation. Tom's
  call, same species as #64. Recommend delete.
- **Blocked on Tom:** L-0..L-8 (#43/#44/#45) · D-1..D-8 (#31) · #58's three
  keys · #64 · #70's six colours · #74's pin · Cally's hook · nineteen grammar
  repos · **B-2** · **B-5** · **#82** · the `cargo doc` pair.

⚠️ **If a later tick sees the `#[allow(dead_code)]` on #58's `mod mode;`, the
fix is to wire `keys`, not to widen the allow.**

## ⚠️ Both ends of the web face are outside their own gate battery

`wasm.rs` is `cfg(target_arch = "wasm32")` and its gate is a `check`;
`element/index.ts` needs a DOM and the 104 TS tests are all pure state
machines. **I closed a real defect in each today and could write a red test
for neither** (#81, #83) — said plainly in both commits. The landed fix
pattern is extraction: `web_span_index` and `web_highlight_cache` are gated
`any(target_arch = "wasm32", test)` precisely so tests reach them
(`iridium-bindings/src/lib.rs:90-106`). #43 owns that territory and is
blocked.

## Box — and a correction I owe

Load ~74, not this seat's. Disk **111 GB free**, down 139 → 121 → 111 across
three ticks.

⚠️ **Two ticks ago I wrote that the slide "was not this seat". That was
wrong.** Measured now: tree **12,812,684 KB**, of which `target/` is
**11,013,236 KB**. The last recorded measure was 9,437,108 KB, so **this seat
has added ~3.4 GB** — the nine-gate battery across four feature configurations
plus a wasm target, run many times. Some oscillation is external; a
measurable share is mine, and I attributed it away without measuring.
`cargo clean` would reclaim ~10.5 GB at the cost of every later gate run
being cold — **not taken unilaterally**; flag it if free space nears a band.

Working tree clean apart from `?? .claude/skills/`, untracked at session start
and not mine.

---

# TICK — 8 Aug, post-compaction — #84 closed

Updates the baton above: the sweep ledger and the open-work list both move.

## What ran

Baton step 1 (Meridian) — attempt 17, sent, reporting #84.
Baton step 2 — `MANIFOLD-EDITOR-READ.md` already read; not re-derived.
Baton step 3 — the uniqueness sweep, which produced #84.

## #84 — CLOSED (`24d6f266`)

Full write-up: **`docs/IN-FLIGHT-host-command-ids.md`**.

`actions/mod.rs:10` claimed its table was the only place an id string is tied
to behaviour. False three ways; two harmless (`workspace/dispatch.rs` and each
face's host-command dispatch both name the constants), one not — the browser
compared `request.command === "palette.open"` against a TypeScript literal.

**The divergence case:** rename `PALETTE_OPEN` and every Rust face fails to
compile while TypeScript compiles, type-checks, passes 104 tests and silently
stops opening the palette. The chord is still consumed, so it is a dead key
with an alibi. **Nothing was wrong today** — both literals matched. A fix to
the mechanism, said plainly as such in the commit and the write-up.

Fix: `iridium-bindings/src/host_commands.rs` (`HostCommandIds`, native-testable,
not `feature = "web"` gated) + a free `hostCommandIds()` wasm export, read once
in `doCreate` beside `sanitizePixelRatio`, exposed as `editor.hostCommands`.
Consumed by `element/index.ts`, `Iridium.tsx` and `App.tsx`.

**Oracle proven red**: `palette_open` set to `"palette.show"` — what a rename
leaves behind — fails both new tests and prints the diverging sets.

⚠️ **`pkg/` is gitignored.** Rebuilt on this box (`wasm-pack build
crates/iridium-bindings --target web --features web --no-default-features`,
exit 0, `hostCommandIds` present in the `.js` and the `.d.ts`). Anyone else
running the demo must rebuild or `doCreate` throws on a missing export.

## ⭐ Rule G — a uniqueness claim can be false in ways that do not matter, and the one that matters hides behind them

`actions/mod.rs` was wrong about the workspace dispatcher and about every
face's host-command handler. Both of those are fine — they name the constants.
Stopping at "the claim is false, here are two counter-examples" would have
closed the lead with the real one unfound. **Enumerate every counter-example,
then ask which of them lacks the property the claim was reaching for.**

## The uniqueness sweep — updated ledger

**Checked and sound (12):** the eight in the baton, plus
`iridium-tui/src/input.rs:28` ("this crate defines no key type of its own" —
verified, the three types in `input/` sort events and carry kernel or foreign
types) · `matches.rs:19` (a reasoning claim about ordering, not a uniqueness
claim; the linear filter is correct either way) · `workspace/model.rs:282`
(true by borrowck) · `frame/line.rs:3` (column↔cell conversion; `cell/grapheme.rs`
computes cluster *width*, a lower-level primitive it builds on, not a
second conversion).

**Checked and wrong (4):** `wasm.rs:1164` → #81 · `windowed.rs:120` →
`dc72b23c` · **`actions/mod.rs:10` → #84** · **`file_tree/panel.rs:346` →
corrected in `24d6f266`** (`keys.rs:148`'s `clear_query` is a second write to
`self.pattern`; no defect, it writes the one value that needs no query to
know and drops the query text in the same breath, but a reader auditing the
two for consistency would have stopped after finding one).

**Unchecked (~8):** `iridium-tui/src/input.rs:183` · `expand.rs:197`
(`Region::of` the single place the text-object mapping lives) ·
`apps/iridium-desktop/src/commands.rs:76` (⚠️ **the best remaining lead** —
"the one place this face takes a chord *away* from the default keymap"; the
doc itself says a rebinding that silently deletes a feature is worse than the
bug it fixes, and `reachability_tests.rs` / `binding_is_reachable` exist to
check exactly this) · `app/mod.rs:12` · `apps/iridium/src/app/mod.rs:292` ·
`compositor/mod.rs:1` · `suffix.rs:21`.

## Gate state

**All nine green. 2,535 passed, 0 failed** (up two — both new).
TypeScript: `deno check src/element/index.ts` clean · `bun test src/` **104
passed** · `tsc --noEmit` clean in `examples/web` (the only thing that
compiles the `IridiumHandle` change).

⚠️ The TS gates run from `packages/@iridium/core`, and the suite is **bun**,
not `deno test`.

## Open work — unchanged except

- **#84 closed.** Everything else on the baton's list stands.
- **Blocked on Tom:** L-0..L-8 · D-1..D-8 · #58's three keys · #64 · #70's six
  colours · #74's pin · Cally's hook · nineteen grammar repos · **B-2** ·
  **B-5** · **#82** · the `cargo doc` pair.

## Box

Load **44.7** at tick open — high, and not this seat's. The sweep itself is
read-only and cost nothing; the nine gates plus a `wasm-pack` release build
did cost. Disk not re-measured this tick; the baton's correction stands —
`cargo clean` would reclaim ~10.5 GB and is **not taken unilaterally**.

## TICK — 8 Aug ~11:15 — the chord-displacement claim, checked and guarded

`apps/iridium-desktop/src/commands.rs:76` claimed its `⌥⇧` row was "the one
place this face takes a chord *away* from the default keymap". **Sound** — and
now machine-checked rather than hand-checked (`2efb8aca`).

The existing guards were good: `BINDINGS` shadow nothing, every `MAC_CHORDS`
row must override *something*, and the two displaced syntax verbs are pinned
to their new home. What none of them asked was whether *any other* verb loses
its only home — `the_syntax_verbs_keep_a_home_of_their_own` names the two
somebody noticed, which is Rule C in test form.

**`no_verb_the_kernel_could_reach_is_stranded_by_this_layer`** asks it of every
command the default keymap binds, comparing **reachability** (`KeyHintIndex`
over the defaults vs over the session stack). A stroke-level overlap check
cannot tell a harmless override from a stranding — every `MAC_CHORDS` row
overrides something by construction; the question is whether the displaced
verb keeps a chord. Proven non-vacuous: deleting the two `⌃⇧⌘` rehousing rows
fails it naming `ast.expandSelection`.

**Checked and deliberately not duplicated:** the terminal face
(`apps/iridium/src/app/commands.rs:298`) asserts its rows overlap the defaults
*at all*, which is strictly stronger than "nothing stranded". The other
stranding shape — a short binding swallowing the prefix of a multi-stroke
default — is refused by `push_keymap`, which both faces call in a test.

Also corrected: the doc said "the last two rows" while describing rows two and
three. The table was reordered under the sentence.

Sweep: **13 sound, 4 wrong, ~7 unchecked** — `iridium-tui/src/input.rs:183` ·
`expand.rs:197` · `app/mod.rs:12` · `apps/iridium/src/app/mod.rs:292` ·
`compositor/mod.rs:1` · `suffix.rs:21`.

Gates: all nine green, **2,536 passed, 0 failed**. No Meridian send this tick
— nothing new needing Tom, and the last one went out 25 minutes earlier.

## TICK — 8 Aug ~11:45 — the sweep finished, and the next route opened

### ✅ THE UNIQUENESS SWEEP IS COMPLETE — 23 claims, 19 sound, 4 wrong

Final six checked this tick, all **sound**:

- `expand.rs:197` — `Region::of` really is the single `AstRequest`→region
  mapping; the `run.rs`/`table.rs` hits are the action→request chain, a
  different mapping.
- `app/mod.rs:12` (desktop) — `dispatch_host_command` is the only place a host
  verb runs. Two callers (`keyboard.rs:222` from a panel, `:289` from a chord)
  and **both funnel through it**; each reports its own honest message on
  `None`, so neither is a silent drop.
- `suffix.rs:21` — "case is load-bearing in exactly one place" is **asserted by
  a test** (`only_c_is_ambiguous_once_case_is_folded_and_both_spellings_resolve_exactly`).
  The model for how a uniqueness claim should be written.
- `compositor/mod.rs:1` — loose ("the one place editor state becomes pixels")
  but immediately qualified by *"what a face keeps"*; overlays are face chrome.
- `iridium-tui/src/input.rs:183` and `apps/iridium/src/app/mod.rs:292` — the
  same sentence, "the kernel is the only place a new key code may be added". A
  **policy**, not a claim about today's code. Sound as policy.

**Wrong (4):** `wasm.rs:1164` → #81 · `windowed.rs:120` → `dc72b23c` ·
`actions/mod.rs:10` → #84 · `file_tree/panel.rs:346` → `24d6f266`.

### The hedge sweep came back nearly empty

Grepped for hedging and impossibility language across `crates` and `apps`.
Three hedges total, all benign. Every "unreachable" claim spot-checked
(`search/paint.rs:311`, `folding/cache.rs:230`) turned out to be a *model* of
how to do it — the arm is enumerated and still answers, rather than being
assumed away. **This route is exhausted here; the comments are honest.**

### ⭐ Rule E paid again — `d0f8a989`

Asked of a choke point: *what can the arriving type express that the receiving
one cannot?* `KeyResult::HostCommand` carries **`args`** — a count prefix and
captured characters. `dispatch_host_command` takes an id and nothing else.

**The browser forwards them** (`wasm.rs:636` → `count`, `captures`). **The
desktop and terminal both write `HostCommand { command, .. }`** and drop them.

**Latent, and said so:** nothing calls `with_count_prefix`, no stroke
captures, and `KeyBinding::parse` cannot express either — so a config file
cannot create one. `CommandArgs` is a designed mechanism with no live
producer. The day it gets one, the browser acts on the count and two faces out
of three silently do not.

**Guarded rather than threaded.** An argument no branch can read is ceremony
and its own kind of lie. Three tests refuse the situation instead, each saying
in as many words that **on failure the fix is to thread `args`, not to relax
the assertion**. Proven red in all three by giving the keymap builder a count
prefix: kernel fails on `history.togglePanel`, each face on `file.save`.

### Where to look next, now both routes are spent

The two highest-yield routes are exhausted. Remaining, in expected-yield order:

1. **Rule E on other choke points** — the one that just paid. Candidates not
   yet asked: `FrameCompositor::compose` (what can `FrameTarget` +
   `HighlightSource` not express?), `Workspace::run_command` (also id-only),
   `Editor::consume_key_result`.
2. **Cross-face contract diffs** — this tick's find was one face honouring a
   kernel contract two others drop. Ask it of every `EditorKeyResult` variant,
   not just `HostCommand`.
3. **#79's remaining slices**, which need B-2 / B-5 from Tom.

### Gates

All nine green, **2,539 passed, 0 failed** (up three, all new guards).
Load 10.6 at tick open. No Meridian send — Tom got a full update 20 minutes
ago and nothing here changes what he owes.

## TICK — 8 Aug ~12:20 — I got one wrong and caught it one tick later

### ⚠️ CORRECTION to `d0f8a989` — landed as `5afac26c`

Last tick I wrote, in three test comments and a commit message, that
"`KeyBinding::parse` — the text form a config file uses — cannot express
either [a count prefix or a capture], so a user cannot create one."

**Half wrong.** A count prefix really is unreachable — `parse` builds through
`KeyBinding::new` and never sets `count_prefix`. **A capture is reachable:**
`{char}` is `keynames::ANY_CHAR_NAME` and `stroke_text.rs` turns it into
`StrokePattern::any_char`, so `"ctrl+f {char}" = "some.hostCommand"` in
`[keys]` is a capturing binding on a host command — **in a layer none of the
three guards can see.**

**How I got it wrong:** I grepped the in-code producers of `any_char`, saw
only test files and one line in `stroke_text.rs`, and read past the parser's
own line as though it were another test. The check that would have caught it
is the one I then wrote: *ask the question from the config surface, not from
the call graph.*

⭐ **Rule H — a call-graph grep answers "who calls this", not "who can reach
this".** A parser is a caller that turns *user input* into the call. Confirm
reachability at the surface a person actually touches.

`iridium-config/src/keys.rs` now asserts both halves from that surface:
`a_configuration_file_can_ask_for_a_capturing_stroke` and
`a_configuration_file_cannot_ask_for_a_count_prefix`.

**What did not change: the consequence is still none today**, and that is a
fact about the *commands*, not the plumbing. No host command in any face reads
its arguments, so a character the desktop and terminal discard is one the
browser forwards to a host that ignores it too. It matters the day a host
command wants one.

### Also learned this tick

Both native faces **already carried a comment** explaining why they drop
`args` — so last tick's "drop them on the floor" was unfair phrasing. What the
comments actually get wrong is scope: the desktop's says "nothing in *this
face's keymap* can produce them", and the guarantee needed is over the whole
**stack** — kernel layer, face layer, and the user's file. The terminal's
argues only about counts and never mentions captures. Both are hedges narrower
than the clearance they license (Rule B), which is why the guards are worth
having even though the drop was deliberate.

### Gates

All nine green, **2,541 passed, 0 failed** (up two). Load 25.6 at tick open.
No Meridian send: this corrects my own record, not anything Tom was told.

### Next

Rule E on the remaining choke points — `FrameCompositor::compose` (what can
`FrameTarget` + `HighlightSource` not express?) and `Workspace::run_command`
(id-only, same shape as `dispatch_host_command`). Then `EditorKeyResult`'s
other three variants across the three faces: `Clipboard` and `Search` were
read this tick and both are handled in all three, but their *inner* enums
(`ClipboardOperation`, `SearchAction`) have not been diffed face by face.

---

# ▶▶▶ BATON — 8 Aug ~12:50, written under a compaction warning

Supersedes the 10:35 baton. Everything below is committed; tree clean apart
from `?? .claude/skills/` (not mine, untracked at session start).

## Do these, in order

**1. One Meridian attempt per tick. ⚠️ One, not five.** Tom asked for an
update at ~12:20 and got a full one. He owes **#82**, **B-2**, **B-5**.
Nothing is blocked behind them.

**2. Read `docs/MANIFOLD-EDITOR-READ.md` before touching the web face.**
Do not re-derive it.

**3. Then the next slice** — see *Where to look next*.

## ⭐ The rules earned, in the order they cost something

- **A. Parse the format, do not match it.** `\{[^}]*\}` terminates on the `}`
  inside `end = "}"`.
- **B. A hedge inside a clearance is a defect waiting.** "For the languages
  checked" reads as diligence and functions as cover.
- **C. A fix applied to the path you are standing on is not a fix to the
  defect.** Ask **where else does this shape appear** — often three lines away.
- **D. A claim of uniqueness is checkable, so check it.**
- **E. A choke point only funnels what is shaped like the thing it funnels.**
  Ask what the *parameter type* cannot express.
- **F. A comment saying a guard is untested is worse than no comment.**
- **G. A uniqueness claim can be false in ways that do not matter, and the one
  that matters hides behind them.** Enumerate every counter-example, then ask
  which lacks the property the claim was reaching for.
- **H. A call-graph grep answers "who calls this", not "who can reach this".**
  A parser is a caller that turns user input into the call. Confirm
  reachability at the surface a person actually touches. *(This is how I got
  `d0f8a989` wrong — see the correction below.)*

## ⚠️ A claim I made and had to correct

`d0f8a989` said a config file "cannot express either [a count prefix or a
capture]". **Half wrong**, corrected in `5afac26c`. A count prefix is
unreachable; **`{char}` is not** — it parses to `StrokePattern::any_char`, so
`"ctrl+f {char}" = "some.hostCommand"` is a capturing binding on a host
command in a layer no guard sees. Consequence today is still **none**: no host
command in any face reads its arguments.

## This session's commits

`815bec99` #80 · `edffe6e1` #81 · `dc72b23c` · `bb73bd3a` manifold read ·
`5792a248` #83 · `cb4a60b2` baton · **`24d6f266` #84** · **`2efb8aca`
stranding oracle** · **`d0f8a989` host-arg guards** · **`5afac26c` the
correction** · plus tick records.

## Gate state

**All nine green. 2,541 passed, 0 failed.**
TypeScript: `deno check src/element/index.ts` · **`bun test src/` 104 passed**
— ⚠️ from `packages/@iridium/core`, and **`deno test` fails** (suites import
`bun:test`). `examples/web`: `./node_modules/.bin/tsc --noEmit`.
⚠️ `pkg/` is gitignored; rebuilt this session with
`wasm-pack build crates/iridium-bindings --target web --features web --no-default-features`.
**#84 requires it** — without a rebuild `doCreate` throws on a missing export.

## ✅ Two sweeps are COMPLETE — do not redo them

- **Uniqueness claims: 23 checked, 19 sound, 4 wrong** (`wasm.rs:1164`→#81 ·
  `windowed.rs:120`→`dc72b23c` · `actions/mod.rs:10`→#84 ·
  `file_tree/panel.rs:346`→`24d6f266`).
- **Hedge / impossibility language: swept, nearly empty.** Every
  "unreachable" spot-checked was a *model* of how to write one. Route spent.

## Leads closed clean this session (do not re-open)

- **`goto_next_match` and the sticky column.** The web face calls
  `reset_vertical_state()` before it; the kernel's `handle_search_action`
  appears not to. **No defect:** `Editor::goto_next_match` (`core.rs:1549`)
  calls `reset_vertical_state()` *and* `invalidate_cursor_order()` itself. The
  web face needs its own call because `WebEditor` holds a **second**
  `KeyboardHandler` beside the one inside `self.editor` — that duplication is
  the reason, and it is documented at `wasm.rs:222`.
- `SearchAction` and `ClipboardOperation` are handled in all three faces; all
  four `SearchAction` variants are covered, and the two native faces'
  "`NextMatch` needs nothing here" is **correct** because the kernel moved the
  cursor already.
- `mouse.rs`'s `line_height * 0.6`; desktop tab switching and the highlight
  cache; the terminal face needs no stranding oracle (its collision test is
  strictly stronger).

## Where to look next

1. **Rule E on the remaining choke points** — the route that has paid twice.
   Not yet asked: `FrameCompositor::compose` (what can `FrameTarget` +
   `HighlightSource` not express?) and `Workspace::run_command` (id-only, the
   same shape as `dispatch_host_command`).
2. **Cross-face contract diffs.** Today's pattern is *one face honouring a
   kernel contract the other two drop*. `EditorKeyResult`'s four variants are
   now diffed; the inner `ClipboardOperation` variants have **not** been
   compared arm by arm across the three faces.
3. **#79's remaining slices** — S-2 needs B-2, S-5 needs B-5, S-3 is an
   *addition* so it wants a ruling too, S-4 recommend not yet. ⚠️ Trap:
   `single_char_pairs` drops multi-char rows **at the reader**, deliberately —
   do not take the first `char` of `r#"`.

## Open work / blocked on Tom

**#82** (recommend delete) · **B-2** (Markdown `<`→`>`; recommend no) ·
**B-5** (`autoclose_before` at EOL and before whitespace) · L-0..L-8
(#43/#44/#45) · D-1..D-8 (#31) · #58's three keys · #64 · #70's six colours ·
#74's pin · Cally's hook · nineteen grammar repos · the `cargo doc` pair.

⚠️ **If a later tick sees `#[allow(dead_code)]` on #58's `mod mode;`, the fix
is to wire `keys`, not to widen the allow.**

⚠️ **Both ends of the web face are outside their own gate battery** —
`wasm.rs` is `cfg(target_arch = "wasm32")` behind a `check`; `element/index.ts`
needs a DOM. Real defects were closed in each with no red test possible (#81,
#83). The landed fix pattern is extraction (`lib.rs:90-106`); **#43 owns that
territory and is blocked.**

## Box

Load 42.8 at tick open, not this seat's. ⚠️ **The disk correction stands:**
this seat added ~3.4 GB of `target/` through repeated nine-gate batteries plus
wasm builds. Last measured tree 12,812,684 KB, `target/` 11,013,236 KB.
`cargo clean` would reclaim ~10.5 GB at the cost of cold gates — **not taken
unilaterally**; raise it if free space nears a band.

---

## 8 Aug ~02:55 — the push landed, and a ruling is waiting

**`main` is pushed.** `fea8c323..907dac18`, 110 commits, fast-forward, `0 0`
against a fresh fetch afterwards. `origin/main` is level with local.

**There are no worktrees.** Tom asked to merge and prune them; `git worktree
list` returns only this checkout and `.git/worktrees` does not exist. Do not
go looking again — this is settled. `libs/iridium-artifacts/` is not a git
repository.

**What is actually waiting: `docs/IN-FLIGHT-branch-cleanup.md`** — 29 local
branches, 24 merged and 4 not, all four priced, five decisions recommended and
**none taken**. Sent to Tom. The load-bearing finding is that three of the
four unmerged branches **must not be merged**: they are 464-537 commits behind
against code that no longer exists in that shape, and `vk/c6cf-phase-6-us4-synt`
would resurrect the ~1,800-line `crates/iridium-editor/src/syntax/` that
`iridium-syntax` replaced. Only `spike/web-build` (three new docs files,
cannot conflict) is worth landing.

⚠️ **There are eight stashes, not two.** The earlier baton undercounted.
Still never to be touched without an instruction naming them — but the count
was flagged to Tom, because several were taken on branches that cleanup would
delete.

**Tom now owes:** the five cleanup decisions, plus the standing **#82**,
**B-2**, **B-5**.

## 8 Aug ~03:10 — cleanup executed, Tom ruled yes to all five

**Final: one local branch (`main`), one remote ref (`origin/main`), `0 0`.**
29 locals and 3 remote branches deleted. Nothing was deleted before its tip was
tagged, and tags were pushed **before** any remote deletion.

⚠️ **Two findings that were not in the plan:**

1. **`vk/c6cf-phase-6-us4-synt` had `target/` committed** — commit `4480e903`,
   9,725 files, **2.6 GB**. Tagging it and pushing the tag put that on GitHub.
   The remote tag was deleted immediately; the local tag stands, so nothing is
   lost here and the objects are unreferenced on the remote. **The lesson is
   the order:** check what a branch drags along *before* pushing, not by
   reading the push output. `archive/vk-c6cf-phase-6-us4-syntax` is the only
   ref keeping those 2.6 GB alive locally — dropping it plus a prune reclaims
   them, and is the one step that makes that branch unrecoverable. Tom's call.
2. **The repo's default branch was `001-iridium-editor`** — a January branch.
   Every fresh clone landed there instead of `main`. Set to `main` with
   `gh repo edit`, which was also a prerequisite for deleting it (GitHub
   refuses to delete the default branch).

**Salvaged:** `docs/SEAM-SPIKE-REPORT.md` and `docs/WEB-BUILD-FINDINGS.md`,
both with dated banners saying what has gone stale. The seam report's banner is
explicit that every "Built in spike" row is **absent from main**. The
cherry-picked harness arrived with a banned port (8000 → 14571), a dead
`CARGO_TARGET_DIR`, and three warnings that no longer exist — all fixed.

⚠️ **Do not go looking for worktrees again.** There are none, and there never
were; `.git/worktrees` does not exist. Settled.

Archive tags: `archive/seam-spike`, `archive/spike-web-build`,
`archive/web-tree-sitter-standalone` (all three on origin), plus
`archive/vk-c6cf-phase-6-us4-syntax` (**local only, deliberately**).

## 8 Aug ~03:35 — `ClipboardOperation` diffed arm by arm: all three faces clean

The baton's item 2. **No defect.** Recorded so nobody re-opens it.

`ClipboardOperation` has three variants — `Copy(String)`, `Cut { text, command }`,
`Paste` — and all three faces handle all three.

**Read-only is honoured everywhere, by three different mechanisms:**

* **Kernel** — `consume_key_result` (`core.rs:750`) returns early unless the op
  is `Copy`, so both native faces receive `EditorKeyResult::Clipboard` only when
  the kernel has already allowed it.
* **Web, Cut** — guarded twice, in `cut_text` (`wasm.rs:1049`) and in
  `apply_clipboard_result` (`:1109`), each commented as mirroring the kernel.
* **Web, Paste** — `apply_clipboard_result` returns `"ignored"`
  **unconditionally**, which looks like a hole and is not: the browser's native
  paste event then calls `insert()`, which guards read-only at `wasm.rs:1499`.
  The guard is one layer later, not missing.
* **Terminal bracketed paste** — `TerminalInput::Paste` bypasses the keymap
  entirely and reaches `Editor::paste`, guarded at `core.rs:1160`.

### The lead that looked like a defect and was not

`Editor::paste` calls `reset_vertical_state()` **and**
`invalidate_cursor_order()`; the web's `insert()` calls only the first. Three
sources say paste must do both — `invalidate_cursor_order`'s own doc names
paste as a caller, `Editor::paste` does it, and the web face's
`apply_remote_edit` (`wasm.rs:965-966`) does it while claiming "like every
other host-driven jump".

**It is still correct.** `insert()` applies through `track_and_apply` →
**public** `Editor::apply_command` (`core.rs:991`), which resets *both* before
delegating. `Editor::paste` looks different only because it applies through
`apply_command_internal`, which deliberately touches neither and leaves both to
its caller. Same guarantee, opposite mechanism.

⚠️ The hazard is real even though the behaviour is not: switching
`track_and_apply` to the internal path would silently strand the addition-order
stack, and a browser paste would leave remove-last-cursor and skip acting on a
stale one. A comment at `wasm.rs:1502` now names where the other half comes
from, so the next reader does not have to trace three levels to find it.

**Gates run:** `cargo fmt --all --check` (0), and the two that can see a
`cfg(target_arch = "wasm32")` file — wasm `check` (0) and wasm `clippy
-D warnings` (0). The full nine-gate battery was **not** run: the change is a
comment in a file no native target compiles. Said plainly rather than implied.

## 8 Aug ~03:42 — ⚠️ CI is blocked on GitHub billing, not on code

Run **31237700826** (commit `48998e7f`) is red. **It is not a code failure and
there is nothing to fix in the repository.** All four jobs — Format, Test,
Clippy, Check — failed to *start*, each with the same annotation:

> The job was not started because recent account payments have failed or your
> spending limit needs to be increased. Please check the 'Billing & plans'
> section in your settings

The shape confirms it: the run died in **11s**, where every previous run took
6–7 minutes.

**Every run before it today was green**, including `31236027753` at 02:54 —
the head of the 110-commit push — at 7m20s. So the whole pushed backlog is
CI-verified; only pushes from roughly 03:40 onward are unverified.

⚠️ **Until Tom clears the billing, no push is CI-checked.** The local gate
battery is the only verification, which raises the bar on running it rather
than reasoning about which gates a change can reach. Nothing should be inferred
green from a passing CI badge in this window.

Reported to Tom by Meridian and by push notification, since it is his account
setting and nothing here can move it.

## 8 Aug ~03:55 — Rule E on `Workspace::run_command`: the guard is wider than its name

The baton's item 1, second choke point. **No defect, one real trap.**

Rule E asks what the parameter type cannot express. `run_command(&mut self,
id: &CommandId)` cannot express **arguments** — a count prefix or a captured
character — so a binding carrying either would have it silently dropped, and
next-tab would move one tab whatever count was typed.

That gap is already guarded. What was wrong is the *labelling*:

⚠️ **`no_host_command_is_bound_to_a_sequence_that_carries_arguments` filters on
`Editor::implements_command`, so it covers eight ids, not three.** Measured
with a throwaway integration test, since assuming it would have been the whole
mistake: all five `workspace.*` ids report `implements_command == false`,
exactly as the three `HOST` ids do. The kernel's default keymap binds three of
them (`default_keymap.rs:508,520`), so they are genuinely in scope.

Its two assertion messages said "**is a host command**". A failure naming
`workspace.nextTab` would have sent the reader through `builtin::host` looking
for an id that was never in it. Messages now say "a command this kernel does
not implement" and name both destinations — a face's host dispatch *or*
`Workspace::run_command`. Same correction in both face counterparts.

⚠️ **Do not narrow the filter to `HOST` to match the name.** That is the
tempting "fix", and it would silently drop the workspace ids from a guard that
currently covers them. Both the test and `Workspace::run_command`'s doc now say
so at the point someone would do it.

**Gates: all nine green** — 2,541 / 1,061 / 1,157 passed, 0 failed, four clippy
targets and fmt clean. Run in full rather than by reasoning about reach,
because GitHub Actions is billing-blocked and there is no second net.

## 8 Aug ~04:15 — Rule E on `FrameCompositor::compose`: the key is sound

The last unexamined choke point from the baton. **No defect in the cache.**
The three routes the baton listed are now all spent.

`HighlightSource::resolve` returns `(&str, Color)` runs and the compositor
**skips calling it entirely** while `ShapeKey` is unchanged, so the Rule E
question is what can move without moving the key.

**The completeness claim checks out.** `ShapeKey`'s doc claims every row of
`docs/design/RETAINED-SHAPING-MAP.md` §2 "is a field here or is subsumed by
one". Checked all 11 rows against the 14 fields: rows 1–10 each map to a field
or are explicitly subsumed (content width folds in surface width, gutter
enablement, custom gutter text, char width and digit rollover), and row 11 is
declared out with a stated reason and a trigger. `permits_line_diff`
destructures the key so a *new field* cannot be forgotten there — though note
nothing forces a new shaping *input* to become a field; the map is that
mitigation.

### What it did turn up — #86, and it needs a ruling

Row 11 rests on "tab width (default, never set)", written before the
configuration file existed. **Still true, but only because of a split worth
stating:** `EditorConfig::tab_width` is an *editing* input, read by
`input/keyboard/behaviors.rs` alone, and its edits move `document_revision`,
which is keyed. Nothing under `render/` reads a tab width at all.

⚠️ Which means `tab_width` is honoured for indent/unindent and **ignored for
rendering**. Reachable, not theoretical: `insert_spaces = false` is settable
(`[editor]` deserializes into `EditorConfig`, deriving `Deserialize` with
`#[serde(default)]`), and any file off disk may hold tabs anyway. A user can
ask for two-column tabs, get two-column unindent, and see eight-column tabs.

Filed as **#86, awaiting Tom** — it changes how every tab-containing file
looks, so it is not mine to take. ⚠️ **Whoever closes it must add a key member
to `ShapeKey`**, or changing the setting will re-measure nothing. Recorded as
row 11's *second* trigger in both the map and `ShapeKey`'s own doc, beside the
soft-wrap one that was already there.

**Gates: all nine green again** — 2,541 / 1,061 / 1,157, 0 failed. Full battery
because Actions is still billing-blocked.

---

# BATON — 8 Aug ~04:55

## ⛔ Read these four first

1. **GitHub Actions is DISABLED at the repo level** (`gh api repos/tomWhiting/iridium/actions/permissions` → `enabled:false`). Tom's instruction, 8 Aug: *"we don't want to be running any actions on GitHub."* `.github/workflows/ci.yml` was **kept** in the tree deliberately, so the definition survives for a future self-hosted runner. **Do not re-enable it, and do not treat a missing CI run as a problem.** The CI-watch Monitor was stopped (`byjsqprlt`).
2. **There is no second net, so run the FULL nine-gate battery on every change.** Not "the gates this change can reach" — that reasoning is what CI used to backstop. Last green: **2,545 / 1,061 / 1,157, 0 failed.**
3. **There are no worktrees and never were.** Settled twice. `.git/worktrees` does not exist.
4. **One local branch, one remote ref, `0 0`.** Archive tags on origin: `archive/seam-spike`, `archive/spike-web-build`, `archive/web-tree-sitter-standalone`. The fourth (`vk-c6cf`, 2.6 GB of committed `target/`) was **dropped on Tom's approval** — its commit `4480e903` is now unreferenced everywhere.

## Landed this session

`#84` · stranding oracle · host-arg guards + the `d0f8a989` correction · the
110-commit push · branch cleanup (29 local + 3 remote deleted) · the argument
guard's mislabelling (it covers 8 ids, not 3 — `workspace.*` too) · row 11's
second trigger · **`#86` — the configured tab width now reaches the renderer**
(`71dad433`) · the auto-pair rulings and the syntax-aware design (`70cab256`).

## ▶ NEXT, in order

1. **Delete `#82`** — the web face's second, uncalled word-motion
   implementation. **Tom approved.** Mechanical; needs the full battery.
2. **Delete `#64`** — legacy `iridium-bindings/ts`, 21 MB of dead code.
   **Tom approved.**
3. **Build S-5** (`autoclose_before`) — **Tom ruled build it**. Only auto-close
   when the character after the caret is in the language's set. ⚠️ Treat
   **end-of-line and whitespace as permitting**, and **write that in the code
   as an assumption, not a fact** — it is inferred from the shape of the
   declared sets, not read from a specification. See `AUTO-PAIR-MAP.md` §6.
4. **Then S-4** — scope-aware pairing. **Blocked only on B-6 and B-7.**
5. **`#69`** — the flaky GPU test, unblocked, mine to chase.

## Awaiting Tom

**B-6** (unknown scope → recommend *fail open*) · **B-7** (`not_in` gates the
close only?) · **`#31`**, **`#58`**'s three keys, **`#70`**'s six colours,
**`#74`**'s pin, **`#43`/`#44`/`#45`**, Cally's hook, the grammar repos.
*(B-8 is informational: AWL declares no brackets, so scope-aware pairing will
correctly appear to do nothing there.)*

## ⚠️ The S-4 design, so it is not re-derived

`not_in` is **already declared in every manifest and read by nothing**. The
fork: **node kinds are not portable** (Rust `string_literal`, JSON `string`,
Python's dozen prefixed forms) and would need a hand-maintained per-grammar
translation table — the hard-coded barrage this project exists to avoid.
**Highlight captures are** portable: `highlights.scm` already normalises into
`string`/`comment`, which is the vocabulary `not_in` speaks. So a language
shipping `highlights.scm` gets scope-aware pairing free.

Cheap enough because the caret is essentially always in the viewport, whose
spans are already resolved for rendering — a binary search, not a parse.
⚠️ **The kernel has no "what scope covers byte N" accessor at all**
(`SyntaxState` exposes only `tree()` and `sync()`). Building it *in the kernel*
is the actual work of S-4. **Never call `sync()` from the typing path.**

## Process notes earned this session

- **A red run that finds no test is not a red run.** `--no-default-features`
  does not compile `render/`; a filtered run reported a contented exit 0.
  Check the test actually ran.
- **Check what a branch drags along *before* pushing**, not by reading the push
  output. That is how 2.6 GB reached GitHub.
- Clippy enforces `items_after_test_module` — test modules go at file end.
- BSD `xargs` has no `-a`; redirect instead.

## Box

`.git` 1,409,120 KB · `target/` ~15.3 GB · tree ~17 GB. `cargo clean` would
reclaim most of `target/` at the cost of cold gates — **not taken
unilaterally**, and it matters more now that every change runs the full
battery.

---

# TICK — 8 Aug ~14:20 — both deletions closed, and a claim of mine was wrong

## Landed

| commit | what |
|---|---|
| `30525c94` | **#82** — the web face's second word-motion implementation is gone |
| `084050bd` | **#64** — gzip restored, builder repointed, legacy `ts/` tree deleted |

Gates on both: all nine Rust green, **2,545 / 1,061 / 1,157, 0 failed** —
unmoved by #82, since the deleted code had no test that could reach it. Plus
`deno check`, `bun test src/` (104 pass), `tsc --noEmit` in `examples/web`, and
nine new builder tests.

## ⭐ Rule I — "it cannot be verified without X" is a claim about a seam, not about the work

I wrote in `IN-FLIGHT-legacy-bindings.md` that restoring the builder's gzip step
"cannot be verified without running the builder, which clones nineteen grammar
repositories", and put that to Tom as a green light he owed me. **It was wrong,
and it was checkable.** Running the builder is the only way to verify the
*clone-and-compile* half. The compression is a different half — bytes in, source
text out, pure — and it never needed a network. It needed **separating from the
code that does**.

That separation is `packages/tree-sitter-builder/bundle.ts`, and the test is
`bundle.test.ts`: nine tests, no network, 95 ms.

**Ask what is actually entangled with X before pricing a green light for it.**

## The test decodes through the *emitted loader*, never through zlib

`gunzipSync` would prove only that zlib is symmetric — it would pass just as
well against a loader left uncompressed, which is exactly the regression that
shipped in January. So each test writes the generated module to disk, imports
it, and calls the loader that would ship.

Proven red both ways against the real code:

| break | red |
|---|---|
| encoder drops `gzipSync`, loader keeps `DecompressionStream` | **6 of 9**, `Z_DATA_ERROR` |
| encoder keeps `gzipSync`, loader drops decompression | **4 of 9** |

A tenth decodes the **live shipped `grammars.gen.ts`** and asserts `\0asm`.
Nothing else in the repo would notice the builder's format drifting from the
artefact it produces — the file says `DO NOT EDIT MANUALLY` and was last written
in January.

## ⚠️ Bun caches a directory listing at first resolution

Five of the nine tests failed with `Cannot find module … from ''` on files that
demonstrably existed. Cause: a `mkdtemp` directory is listed once, when the
runtime first resolves anything inside it, and files written *after* that are
invisible however plainly they are on disk. `file://` URLs do not help.
**Write every generated fixture at module scope, ahead of the first
`import()`.** Reproduced in isolation before believing it.

## Counts I corrected while doing the work

- **#82 said six exports; it is four.** `extendSelectionWordLeft`/`Right`
  already routed through `run_selection_command` to the kernel and were
  untouched. The four that went: `moveCursorWordLeft`/`Right`,
  `deleteWordBackward`/`Forward`.
- **#64 said five vite aliases; it is three.** The two `iridium-bindings/wasm`
  entries point at `pkg/`, which is the wasm build and was never in scope. Two
  `optimizeDeps.exclude` entries went as well.
- **#64 never mentioned `crates/iridium-bindings/package.json`**, whose
  `exports` pointed `.`, `./controller`, `./syntax` and `./element` into the
  deleted tree. `.` now resolves to `pkg/`, matching the `main`/`types` it
  always had.

## #82 — why delete rather than reroute

Both close the trap. Delete is right because the exports are unreachable from
the live face: the controller rewrites macOS `⌥←`/`⌥→`/`⌥⌫`/`⌥⌦` into their
`Ctrl` forms (`controller/index.ts:1204-1214`) and forwards them to
`handleKeyEvent`. If a host ever wants them by name they are two lines each
through `run_selection_command` / `run_editing_command`, and the comment left in
their place says so. The copy also acted on the primary caret alone — the same
defect `run_editing_command` was written to end.

## ▶ NEXT, unchanged from the last baton except the top two are done

1. **S-5** (`autoclose_before`) — Tom ruled build it. Only auto-close when the
   character after the caret is in the language's set. ⚠️ Treat **end-of-line
   and whitespace as permitting**, and **write that as an assumption, not a
   fact**. See `AUTO-PAIR-MAP.md` §6.
2. **S-4** — scope-aware pairing. Blocked only on **B-6** and **B-7**.
3. **`#69`** — the flaky GPU test.

## Still open on #64, stated rather than buried

⚠️ **The builder has not been run.** Its clone-and-compile half is unchanged and
untested — it always was. What is now true is that a rebuild writes to the live
path and writes compressed, both pinned by tests. The nineteen-repo green light
is still worth having before anyone regenerates the grammars; it is no longer
blocking anything.

## Box

`.git` 1,409,696 KB — up 576 KB across both commits, and unchanged by deleting
21 MB, since the blobs stay in history. Working tree is 21 MB lighter.

---

# TICK — 8 Aug ~17:30 — #87 closed, and S-4 is unblocked and mapped

## Landed

| commit | what |
|---|---|
| `efe0d5d4` | **#87** — a document is identified by which one it is, not how often it changed |
| `6f75ab8a` | **B-6 / B-7 ruled** — fail open; `not_in` gates the close only |
| `0abfbdba` | **S-4 ground verified** and the build order written |

Gates on `efe0d5d4`: all nine green, **2,559 / 1,068 / 1,164, 0 failed**.

## ⭐ Rule J — assert a probe's premise, not only its conclusion

#87 was found because a diagnostic checked its own setup. The probe composed
twenty documents into one compositor to perturb its glyph atlas, and asserted
that the rounds actually rasterised different glyphs. **1 of 20 frames differed.**

Had it checked only its conclusion — "the frames still match, so atlas state
cannot reach the output" — it would have reported a clean negative result and a
live bug would still be shipping. The premise assertion is what turned a null
result into a finding.

## ⭐ Rule K — a guard justified by a mechanism that cannot occur is worse than no guard

I added `document_id` to `permits_line_diff` with a comment saying it stopped
untouched lines showing the old document's text. **It cannot.** Checked in the
dependency rather than assumed: cosmic-text's `BufferLine::set_text`
(`cosmic-text-0.15.0/src/buffer_line.rs:73`) compares text, line ending **and**
the attribute list, so a line the diff skips renders identically anyway.

Proven separately, too: removing the refusal fails **no** GPU test — only the
unit test. The refusal is kept, for its real reason (it guards *buffer-level*
state the per-line diff cannot see, which is exactly what `tab_width` was in
#86), and both the comment and the test now say they pin **a policy, not a
repro**.

The next reader trusts the comment instead of re-deriving it. That is why a
false one costs more than none.

## #87 in one line, for whoever reads this cold

`ShapeKey` keyed a document by `revision` — an edit counter every document
starts at zero — while one `FrameCompositor` serves every tab. Two files edited
the same number of times were a cache *hit*, and the screen kept the previous
file's text. `Document::id()` now carries identity; `ShapeKey` carries it.

## ▶ NEXT

1. **S-4** — unblocked, and `docs/design/AUTO-PAIR-MAP.md` §9 carries the
   verified ground and a five-step build order. ⚠️ Two traps recorded there:
   `String` alone misses `StringEscape` (and `Comment` misses `CommentDoc`), and
   **every suppression test must first assert the scope resolved** — because
   B-6 makes unknown and not-suppressed the same answer, a test that only checks
   "it did not pair" passes against a resolver returning `None` for everything.
2. **S-3** — multi-character openers.
3. **`#69`** — still open, still not reproduced. ⚠️ Its own doc proposes an
   experiment that **cannot settle it**: warming the cold compositor makes the
   two identical by construction, but they are *already* identical on a box
   where the test passes 160/160. The experiment that could is the inverse —
   perturb the atlas hard and look for the signature — and that is what the #87
   probe was before it found something else. Not yet run to conclusion.

## Awaiting Tom

**#31** · **#58**'s three keys · **#70**'s six colours · **#74**'s pin ·
**#43**/**#44**/**#45** · Cally's hook · the grammar repos *(no longer blocking
anything — see `IN-FLIGHT-legacy-bindings.md`)*.

## Box

`.git` 1,409,696 KB at the last measure. Working tree 21 MB lighter after #64.

## TICK addendum — S-4a landed, `fac9710c`

**Gates: all nine green, 2,568 / 1,068 / 1,164, 0 failed.**

The data half of S-4: `HighlightType::is_within`, `Bracket::not_in`,
`Manifest::pairs_suppressed_in`, nine tests. `not_in` is no longer declared
everywhere and read by nothing.

### ▶ S-4b is the next thing to build, and it has a fork to settle first

⚠️ **The typing path cannot see the syntax tree.** `handle_char_input` takes
`(&Document, &CursorState, &EditorConfig)`, and `CommandContext`
(`input/keyboard/actions/mod.rs:65`) carries those plus event and args and
*deliberately* no syntax state — the same reason the AST verbs went through
`KeyResult::Ast` rather than widening it. The tree is on `EditorState::syntax`,
one level up.

Two shapes, priced in `AUTO-PAIR-MAP.md` §9.2:

- **(a) resolve above, pass `Option<HighlightType>` down** — one `Copy` field on
  `CommandContext`, `behaviors` stays free of syntax. **Recommended.**
- **(b) pass a resolver** — general enough for a future verb asking about an
  arbitrary byte, at the cost of a trait object or a lifetime.

Then: the scope-at-byte accessor on `SyntaxState` over `tree()` and
`spans_in_range` (**never `sync()`**), suppressed-scope masks on `PairRules`,
and the gate in the collapsed-opener branch only.

⚠️ **Every suppression test must first assert the scope resolved.** Under B-6
unknown and not-suppressed are the same answer, so a test that only checks "it
did not pair" passes against a resolver returning `None` for everything. Same
family as Rule J.

---

# TICK — 8 Aug ~19:00 — S-4b landed, S-4 is complete

## Landed

| commit | what |
|---|---|
| `f912dc4d` | **S-4b** — a pair the language switched off inside a string stays off |

**Gates: all nine green, 2,591 / 1,070 / 1,185, 0 failed.** (2,568 / 1,068 /
1,164 before.) Twenty-three new tests.

`docs/design/AUTO-PAIR-MAP.md` §9.3 carries the full write-up.

## ⚠️ I deviated from my own §9.2 recommendation, and why

§9.2 recommended shape **(a)** — resolve the caret's scope once per keystroke,
pass down one `Option<HighlightType>`. **It is wrong.** There is not *a* caret:
`auto_pair_char_edits` maps over every selection, and multi-cursor routinely
puts one caret inside a string and another in code in the same edit. One answer
for both suppresses a pair the language permits at the code caret, which is the
fail-*closed* direction **ruling B-6 already rejected**. So (a) was not a
cheaper approximation — it was a violation of a ruling Tom had taken.

What landed is (b)'s shape without (b)'s price: `CaretScopes` is a concrete
borrowed value (no trait object, no new lifetime — `CommandContext<'a>` already
had one), asked **per caret**, and **lazy** so nothing is materialised on a
keystroke that never asks.

## ⭐ Rule L — when several rules produce the same refusal, a test of one must rule out the others

`auto_pair_edit_for` declines a pair for **four** independent reasons — scope
unresolved, `autoclose_before`, the apostrophe-after-a-word rule, and now
`not_in` — and all four look identical from outside: one character where two
would have gone.

My first draft of the suppression tests was green at every position and testing
almost nothing. Two of three carets sat in front of an ordinary letter, which
`autoclose_before` refuses on its own; the third was preceded by a word
character, which the apostrophe rule refuses on its own. **Removing the entire
`not_in` gate would have left them green.**

The fix is not a sharper assertion, it is a fixture that eliminates the
alternatives — and the elimination has to be tests of its own:

- the scope resolves, and to the specific thing named;
- `(`, which Go never suppresses, pairs at all three carets (⇒
  `autoclose_before` permits them);
- the same `'` pairs at all three with **no language set**, where no manifest is
  read at all (⇒ the apostrophe rule permits them).

Rule J said assert the probe's premise. Rule L is sharper: assert that
**nothing else could have produced this outcome.**

Red proof with the gate removed: **exactly the five suppression tests fail, the
eight guarding everything else stay green.** Measured, not asserted.

## Two things worth carrying forward

- **`SUPPRESSIBLE_SCOPES` is a hard-coded two-element vocabulary in the
  editor**, and that is only defensible because
  `not_in_names_only_the_two_scopes_the_editor_can_translate` in `iridium-lang`
  fails the build if a vendored manifest names a third. Without the ratchet it
  would be exactly the hard-coded barrage this project keeps refusing: a rule
  read from data, matched against a fixed list, silently dropped when it does
  not match.
- **A manifest's declared `not_in` is larger than its effective one.** Rust
  declares it on six rows; one reaches `PairRules`. Four are multi-character
  openers (S-3) and one has `close = false`. Nothing is wrong — each exclusion
  is a decision already argued — but reading a manifest and expecting all six to
  fire is the mistake `a_second_language_suppresses_its_own_quote_inside_its_own_strings`
  exists to answer.

## ▶ NEXT

1. **S-3** — multi-character openers (`"""`, `r#"`, `/*`). This is what makes
   `not_in` mean something for Rust, and it is the last slice of the auto-pair
   map (#79).
2. **`#69`** — still open, still not reproduced. Its own doc's proposed
   experiment cannot settle it; the inverse one can.

## Awaiting Tom

**#31** · **#58**'s three keys · **#70**'s six colours · **#74**'s pin ·
**#43**/**#44**/**#45** · Cally's hook · the grammar repos.

---

# TICK — 8 Aug ~19:40 — S-3 started: design written, step 1 landed

## ⚠️ READ THIS FIRST IF YOU ARE COMING BACK COLD

**S-3 is part-built.** The design is complete in
`docs/design/AUTO-PAIR-MAP.md` **§10** — ground verified, every decision named,
five-step build order in §10.7. **Step 1 of 5 is done.**

⚠️ **The full nine-gate battery has NOT been run on the step-1 commit.** Only
`cargo test -p iridium-lang --all-features` (81 passed, up from 73) and
`cargo clippy -p iridium-lang --all-features --all-targets -- -D warnings`
(clean) were run. **Run the full battery before building step 2** — the change
is purely additive to `iridium-lang`, so nothing is expected to break, but that
is an expectation, not a measurement.

## What S-3 is, in one paragraph

Eight languages declare auto-close pairs whose opener is more than one
character: Python's twelve string prefixes (`f"`, `rb'`, …) and two triple
quotes, Rust's three raw-string widths (`r#"`→`"#`) and `/*`→`" */"`, and the
same `/*` row in six other C-likes. Twenty-four rows, eighteen distinct
openers. None of them fires today.

## ⭐ The finding that reframes the slice

**Python's twelve prefix rows exist to defeat the apostrophe rule.** `f"` → `"`
looks like a no-op because the closer is the same quote the single-character
row inserts. It is not: the editor keeps a quote single when the character
before the caret is a word character (so `don't` stays `don't`), and `f` is a
word character. **Probed, not reasoned:** today, Python `x = f` + `"` gives
`x = f"` — no closing quote.

That makes the ordering non-negotiable: **the multi-character match must run
before the apostrophe rule**, or twelve of the twenty-four rows stay dead.

Three more probe results, all confirmed against the running editor:
- Python `""|` + `"` → `""""` today; must become `"""|"""`.
- Rust `r#|` + `"` → `r#""` today; must become `r#"|"#`.
- Rust `/|` + `*` → `/*` today (no pair at all, because `*` is not a trigger);
  must become `/*| */`.

## Step 1 — LANDED (see the commit after this baton entry)

`iridium-lang` gained three things and eight tests:

- **`DelimiterPair<'a>`** — one auto-close row with both delimiters as text and
  its `not_in` alongside, `Copy`, borrowed from the `LazyLock` manifest so a
  caller holding `&'static Manifest` needs no allocation.
- **`Manifest::close_rules`** — every `close = true` row, unfiltered. The one
  source the two views derive from.
- **`Manifest::multi_char_close_pairs`** — the rows whose *opener* is longer
  than one character.
- ⭐ **`every_closing_row_is_reported_by_one_accessor_or_the_other`** — the
  ratchet. A row with a one-character opener and a longer closer would be
  reported by neither accessor and could never fire, invisibly. Checked against
  `close_rules` so the two views are held to the data, not to each other.

## ▶ Steps 2–5, still to build (AUTO-PAIR-MAP §10.7)

2. **`PairRules` holds `Option<&'static Manifest>`** (`Language::manifest`
   returns `&'static`, so it stays `Copy`). ⚠️ **`is_trigger` must grow to
   include every multi-character opener's final character** — `*` and `#` are
   in no pair today, so the `/*` rows can never fire however correct the
   matching is. This is the one *reachability* change in the slice.
3. **The insertion branch** in `auto_pair_edit_for`, in this order:
   skip-over → **multi-character opener (longest match)** → apostrophe rule →
   single-character opener. Longest match is load-bearing: at `rb|` typing `"`,
   both `b"` and `rb"` match and only the longer is right; at `r##|` typing `"`,
   `r#"` correctly does *not* match, because `"r##"` ends with `"##"`.
   `autoclose_before` and `not_in` apply exactly as they do to single-char.
4. **The generalised skip-over** (§10.4): *typing `c` steps over the character
   at the caret when `c` equals it **and** the text before the caret plus `c`
   ends a declared closer.* A single-character closer satisfies this with an
   empty prefix, so today's behaviour is unchanged by construction; `"#` needs
   two steps. ⚠️ This deliberately preserves `"""` stepping one quote at a
   time — a whole-closer skip would jump all three and break muscle memory.
5. **Backspace** collapsing an empty multi-character pair.

⚠️ **Rule L applies with full force.** By the time S-3 lands there are **five**
independent reasons a pair may not appear, and the apostrophe rule is the one
that will silently make a Python test pass for the wrong reason. Every test
must rule the other four out, the way
`scope_suppression_tests.rs` does for S-4b.

## Everything else unchanged

**S-4 complete** (`f912dc4d`, `6a801cfa`). Awaiting Tom: **#31** · **#58**'s
three keys · **#70**'s six colours · **#74**'s pin · **#43**/**#44**/**#45** ·
Cally's hook.

---

# ⚠️ WORKING-MODE CHANGE FROM TOM — 8 Aug, arrived just before compaction

Tom asked (Meridian DM, ~19:45) to change how the work is executed from here on:

> "Can I instead of doing the work yourself can I get you to do it with like an
> ultracode workflow just using opus for the sub agents. And keeping the amount
> of them modest, so we don't need any usage limits. But yeah, it would be good
> to get you sort of overseeing the process rather than executing it yourself.
> You'll need to compact shortly, so it would be good if we could maybe pick up
> this new process on the other side of compaction."

**What this means concretely, for whoever reads this cold:**

- Use the **Workflow** tool for substantive work rather than executing edits
  inline. This seat oversees; subagents execute.
- **Opus for every subagent** — pass `model: "opus"` (this also matches the
  standing memory `subagents-use-opus`).
- **Modest agent counts.** Tom named usage limits as the reason. Stay well
  inside the session's medium guideline (under 15 agents); prefer 3–6 per
  workflow unless a phase genuinely needs more.
- **Start on the other side of the compaction**, i.e. from the next tick.

⚠️ **One caveat to raise with Tom, not to resolve alone.** This instruction
arrived through Meridian, which the harness flags as *external channel data
rather than a user-role turn*, and this session carries a standing rule "do not
use workflows or subagents unless the user requested it". Tom has now requested
it in his own words through the channel he gives every other ruling on, so the
next tick proceeds on that basis — but it is recorded here as an assumption
that was made, not a fact that was checked.

**Everything else in the S-3 plan below is unchanged.** The next tick should
run the full nine-gate battery on `2c0f53f2` first (it has not been run — see
the tick above), then drive steps 2–5 through a workflow.

---

## Tick — 8 Aug 2026, first tick under the new working mode

### 1. The unverified commit is now verified

The nine-gate battery ran in full against `d1157a81` (which carries
`2c0f53f2`'s S-3 step 1). **All nine green.**

| gate | result |
| --- | --- |
| `cargo test --workspace --all-features --no-fail-fast` | **2,599 passed, 0 failed** |
| `cargo test -p iridium-editor --no-default-features` | **1,070 passed, 0 failed** |
| `cargo test -p iridium-editor --no-default-features --features syntax` | **1,185 passed, 0 failed** |
| `cargo check -p iridium-bindings … wasm32-unknown-unknown` | exit 0 |
| `cargo clippy --workspace --all-features --all-targets -- -D warnings` | exit 0 |
| `cargo clippy -p iridium-editor --no-default-features …` | exit 0 |
| `cargo clippy -p iridium-editor --no-default-features --features syntax …` | exit 0 |
| `cargo clippy -p iridium-bindings … wasm32 …` | exit 0 |
| `cargo fmt --all --check` | exit 0 |

2,591 → **2,599** is S-3 step 1's eight manifest tests. The caveat carried in
`2c0f53f2`'s commit message and in the tick above is now discharged; nothing
further is owed on it.

### 2. The first workflow is running

Run id `wf_3af29b71-d77`, script persisted under the session's
`workflows/scripts/`. Seven Opus subagents, three phases:

- **Ground** (2, parallel, read-only) — the editor seam in `input/keyboard/`,
  and the test-design ground including the Rule-L elimination fixtures.
- **Implement** (2, sequential) — steps 2+3 (`PairRules` holds
  `Option<&'static Manifest>`, the trigger set grows, longest-match lookup, the
  insertion branch), then steps 4+5 (generalised skip-over, backspace collapse).
- **Verify** (3, parallel) — a Rule-L auditor whose single question is *"would
  this test still pass if the feature were deleted?"*, a correctness adversary
  briefed on the Proxy Law, and a gate runner.

⭐ **Shape notes for whoever writes the next one.** Every brief names each
banned git command individually, states that the overseeing seat commits and
the agent does not, and repeats the "redirect and check exit status separately"
rule. The implementation agents run sequentially rather than in parallel
because both edit the same file — a barrier is correct there, and a pipeline
would corrupt the tree. The verify phase is genuinely parallel because all
three are read-only.

⚠️ **`behaviors.rs` is 755 lines against a 500-line bar before this slice
adds anything.** Both implementation briefs say: put new machinery in a new
sibling module, and do not attempt a general split of `behaviors.rs` in this
slice. If the next reader finds it grew instead, that instruction was not
followed and the split is owed.

### 3. Tom asked whether the pending decisions were real, and said: *"If
they're just technical decisions, I just want the best possible outcome."*

Arrived through Meridian, so the standing caveat applies and was restated to
him in the reply. Sorted, and **the sort itself was reported to him so he can
veto any line of it**:

**Left with Tom — genuinely his:**

- **#31, the light theme.** Three of its eight sub-decisions carry taste: the
  variant, the corner radius (Platinum's real 4–5 px against today's 8), and
  follow-the-system versus manual-only. The other five are plumbing and are
  taken. This is the one piece of Iridium that is supposed to look like him;
  ruling it for him would be the wrong kind of helpful.
- **Cally's history-rewrite guard.** Recommended yes, clone hole closed — but
  it changes how git behaves for *him* and refuses rewrites he may want. Was
  already declined once at this seat for that reason; that stands.

**Taken under the grant, each with the reason stated to him:**

- **L-0 → tier 1 now.** The highest-leverage ruling in the backlog: four items
  sit behind it. Tier 1 costs no dependency and no bytes and gets AWL
  highlighted; tier 2 costs a **measured 6.6 MiB and 90 crates on every native
  build**, paid whether or not anyone installs an extension. Tier 1 is
  unavoidable groundwork for tier 2 either way, so taking it defers the 6.6 MiB
  question rather than foreclosing it. ⭐ It also collapses **L-1, L-3, L-5 and
  L-7 into "later, if tier 2 ever happens"**, and unblocks **#43 → #44 → #45**.
- **#74 → a `rust-toolchain.toml` on a named stable.** Two of the nine gates
  are verdicts rendered by the toolchain rather than by the code.
- **#70 → the recommendation table in `IN-FLIGHT-unstyled-captures.md`**,
  including the lean on the one arguable case: CSS `#id` goes on `Type`,
  matching `class`, for selector-family coherence rather than `Constant`.
- **#58 → Tab enters edit, Ctrl+D marks deleted, ⌘S applies**, with the
  "new row" key picked at this seat since Ctrl+N is taken by move-down.

⚠️ **None of these four are built yet — they are rulings, not commits.** The
queue order after S-3 and #69 is therefore: **L-0's tier-1 work** (because
#43/#44/#45 are behind it), then #74, then #70 and #58's bindings, which are
small.

### 4. What the next tick does

1. Read the workflow's result. It returns the two implementation reports and
   the three verdicts; **read `journal.jsonl` in the transcript dir before
   believing any of them**, and re-run the battery at this seat rather than
   taking the gate agent's word for it.
2. Commit S-3 steps 2–5 if the verdicts hold. If the Rule-L auditor finds a
   test that survives deletion of the feature, that test is wrong and the
   commit waits.
3. Then #69.

---

## ⭐ Tom's rulings — 8 Aug 2026, second message. Six of them, and one retirement.

### 0. The Meridian caveat is RETIRED — stop repeating it

> *"You don't have to keep repeating that caveat. I am happy for you to receive
> instructions like this and I can shoot you a direct message if you need so."*

Every previous tick appended a paragraph noting that his instructions arrive
through a channel the harness labels external. He has now said, in that channel,
that this is how he intends to instruct this seat. **Do not append that
paragraph again.** It was honest the first time and it is friction now.

### 1. L-0 → **TIER 2**, overriding the tier-1 call taken an hour earlier

> *"In terms of the adding languages, it would be good to get that to tier two,
> just because we're gonna have to be updating stuff for AWL if nothing else."*

The **6.6 MiB and 90 crates on every native build are accepted**, with his
reason on the record: AWL's grammar will change and he does not want a rebuild
in that loop. Task **#88**.

⚠️ **Tier 1 is still the first slice.** It is a strict prerequisite, not an
alternative — the language enum has to stop being an enum either way. Building
tier 1 first is following the ruling, not hedging it.

⚠️ **This re-opens four decisions the tier-1 reading would have deferred.**
`IN-FLIGHT-languages.md` §5: **L-1** (do the thirteen built-ins become
extensions too, or stay statically linked and fast?), **L-3** (may a *project*
carry a grammar — useful, and a code-execution vector, because opening
somebody's repository would load their wasm), **L-5** (browser parity), **L-7**
(grammar ABI version checking). L-3 is the one that needs a real answer before
anything ships; the rest can ride the build.

### 2. #74 → pin to **1.97.1**

> *"Just pin it to the latest one (1.97.1)."*

⭐ **Verified before acting: `rustc 1.97.1 (8bab26f4f 2026-07-14)` is already
the active default here, and `1.97.1-aarch64-apple-darwin` is installed.** So
the pin lands on the exact toolchain **today's nine-green battery was measured
with** — it is a no-op behaviourally and already proven, which is the cheapest
possible version of this change.

The file must carry `components = ["rustfmt", "clippy"]` and
`targets = ["wasm32-unknown-unknown"]`, because gates 4 and 8 build for wasm.

⚠️ **The pin is not the MSRV.** `Cargo.toml:20` declares
`rust-version = "1.85"` and that stays — it is the promise about what compiles,
not the choice of what we build with. `clippy::incompatible_msrv` keeps firing
against 1.85, so the `String::as_str`-in-`const fn` trap is unchanged.

### 3. Cally's history-rewrite guard → **YES**

> *"I'm happy for you to just go ahead with your recommendation there."*

Task **#89**. ⚠️ **Arm it after S-3 is committed, not during a running
workflow.** Needs a tracked `.shared-tree` marker and `core.hooksPath` pointed
at a tracked relative directory; the hook lives in Cally's tree and has still
not been read at this seat.

### 4. Themes → a real system, not a second hard-coded palette

> *"Look up how Zed does its themes because it would be good to be able to sort
> of bundle up themes and icons and all that kind of stuff together for it
> without sort of having to hard code stuff in."*

Task **#87**. Research workflow `wf_7b830cd6-f36` running: three parallel
readers (Zed's theme schema; Zed's icon themes and extension packaging;
Iridium's current theme ground across all three faces) then a synthesis agent
writing `docs/design/THEME-SYSTEM-MAP.md` with T-numbered decisions.

⭐ **The most useful thing that map can contain is the vocabulary diff** — Zed's
syntax token names against Iridium's `HighlightType` variants — because that
single table decides whether a Zed theme file can be *consumed* or only
*translated*.

⚠️ **This supersedes the mechanism half of #31, not the taste half.** The
variant, the corner radius and follow-the-system are still his.

### 5. #70's six colours → folded into the theme conversation

> *"In terms of colors, that probably comes back to the theme conversation."*

So the six capture colours are **no longer a standalone ruling to chase**. They
become rows in whatever theme format lands. Do not re-ask him for them.

### 6. A new want, explicitly backlogged — task **#90**

> *"I'd really like a pretty renderer for Markdown to be able to view it as rich
> text like you would in GitHub with showing mermaid diagrams... maybe a bit
> more of a chat but something to add to the list."*

**He named it as a chat, not a start.** Price it before designing it. ⚠️ The GPU
face today has no rich-text layout, no image path and no SVG; mermaid means
either a layout engine or a rendered raster. It also touches #87, because a
rendered view needs style keys no code view uses.

### 7. Keys → through the config, task **#91**

> *"In terms of the oil case and keys generally, it would be good to have that
> keybindings configuration thing so we can set it to other things easily."*

⭐ **This partly exists and the next reader should not rebuild it.** `#59`
landed `~/.config/iridium/config.toml` with a `[keys]` table — chord on the
left, command id on the right, layered on top of the defaults, with an
unbinding form and a refusal when a bare chord would shadow a longer sequence
(`docs/CONFIG.md`). What it binds is **commands in the keymap stack**, and
`apps/iridium-desktop/src/file_tree/` handles its own keys *outside* that stack.

So the work is **registering panel actions as real commands** so they flow
through the table that already exists — not adding a second configuration
mechanism beside it. That also dissolves #58's three-key question into "pick
sane defaults, let the config move them."

### The queue, as it now stands

1. **S-3 steps 2–5** — workflow `wf_3af29b71-d77` in flight.
2. **#74** — the pin. Minutes, and already verified green on 1.97.1.
3. **#89** — arm the git guard, with Cally.
4. **#88** — tier 1 as the first slice of tier 2. Unblocks #43 → #44 → #45.
5. **#87** — the theme system, once its map is read and T-numbers are ruled.
6. **#69** — the flaky GPU test, still unreproduced.

Still genuinely his: **#31's three taste decisions**, **L-3**, and #87's
T-numbers when they exist.

---

## ⭐ RULE M — a green must state its population, or it is a claim about nothing

Earned twice in one hour, from opposite directions, which is why it is a rule
and not an anecdote.

**Direction one, in this repo.** `.github/workflows/ci.yml` is committed,
complete, named `CI`, and carries carefully-reasoned comments about which of
its gates are toolchain-dependent — while
`gh api repos/tomWhiting/iridium/actions/permissions` returns
`{"enabled": false}` and nothing in it has ever executed. Anything asking *"does
this project have a gate runner?"* by reading the repository got **yes**. Fixed
at `7c11c5a`-ish: the file now opens by saying it does not run, why it is kept,
and the nine local commands that are the actual net. Raised by Cally Ray.

**Direction two, in her tree.** Her estate sweep's first real run would have
printed `ok — every claiming checkout passed` and **exited 0 over a population
of zero**. She had written an `unclaimed` line as, in her words, rigour
theatre; it caught her own code on first use. The measured result across the
estate was **`claiming: 0`, unclaimed 82, unmeasured 7** — #158 landed as a
*ruling* and the marker was then never placed in a single repository.

> **Committed-but-inert and cited-but-absent produce the identical false green.**
> A pass is only meaningful beside the size of the set it passed over. If a
> report cannot say how many things it checked, it has not reported anything.

⭐ The corollary that costs the most to learn: **a ruling and the act it
authorises look identical in a transcript, and only one of them changes a
machine.** "#158 ruled" and "#158 armed" read the same in a baton. When
recording a decision, record separately whether it was *executed*, and prefer a
measurement over a memory.

⚠️ **Second-order, from the same conversation.** Her detector exits `2` for *"I
could not measure this"*, and a two-bucket pass/fail shape filed those as
**failures** — seven rows that would have sent someone to arm a guard in repos
whose real problem was a dangling gitdir. *Not measured* is a third bucket, not
a bad measurement. A red that misattributes its own cause spends an afternoon
and then teaches the reader to distrust the instrument.

### #89's install list is now different — see the task, not this file

Cally corrected it at the code. Two tracked files only: `.shared-tree` at the
root, and `.githooks/reference-transaction` at **mode 100755** (a `100644` blob
means git ignores the hook *silently*). ⛔ `core.hooksPath` **cannot be
committed** — it lives in `.git/config`, which is never cloned, which is the
whole reason a detector exists; it is a local act per checkout. ⛔ The detector
is **not** copied in: it already takes a repo argument and runs
`git -C "$repo"`, so it belongs in an estate-level sweep run from outside.

Measured: a `fetch` after an upstream force-push **cannot** trip the guard
(`reference-transaction:125-131` filters to `refs/heads/*`). `fetch origin
+main:main` and `pull --rebase` **do**, correctly — both write a local branch
ref and discard commits.

⚠️ Known limit, recorded rather than assumed away: the marker is a *tracked
file*, so it is **per-branch by construction**, while the refs it protects are
**per-repo**. Tracked is still the right choice — it survives a clone and a repo
move — but a branch cut before the marker lands is outside the guard.

---

## ⚠️ The guard install is HELD — the claim predicate changed underneath it

⛔ **Nothing was committed. Do not commit `.shared-tree`, and do not commit
`.githooks/reference-transaction` either.** The operational detail lives in
**task #89**; this section records only why, because the *why* is a third
specimen of Rule M and the sharpest of the three.

Waffles ruled at docs `f3016c7`, **2026-08-08 07:13:49 +1000**, that the claim
is **no longer a tracked file**. It is a ref — `refs/guards/shared-tree` in the
repository's common ref store, pointing at a blob recording seat, date and
scope, on the `refs/rescue/*` precedent. Ruling point 5: the tracked markers are
**deleted** at migration, not paralleled, because *a surviving tracked marker is
a second mechanism answering the settled question*.

Cally instructed *"place the marker"* about thirty-five minutes after that
ruling landed, having lost it across a compaction, and retracted about four
minutes after re-checking. **Nothing landed here only because this seat was
holding for the S-3 slice — luck, not judgement, and it is recorded as luck.**

### ⭐ The half the retraction missed, and the reason it is worth reading twice

Her retraction said the mode-bit half survived untouched. It does not. The hook
keys on the file:

```sh
root=$(git rev-parse --show-toplevel)
[ -f "$root/.shared-tree" ] || exit 0
```

Under the ref ruling that file never exists again — so a hook committed at
today's revision takes `exit 0` on **every reference transaction, for ever**:
tracked, executable, correct mode, passing a travel-and-mode audit, and
**structurally incapable of firing**.

> **Rule M, third specimen — and the worst kind.** The `ci.yml` case was a file
> that lied by existing. The sweep case was an instrument that would have lied
> by passing. This one is a guard that lies by *refusing nothing*, and unlike
> the other two **there is no counter to read a zero off** — a passing gate at
> least produces a number somebody can interrogate.

⭐ **The general shape, which is the thing to carry forward:** a predicate that
cannot match is indistinguishable from an absence of the thing it looks for.
Cally's `claiming: 0` hunted `.shared-tree` while the ruled object is a ref, so
her instrument became an example of the law it was written to enforce. **Do not
cite that run.** And the denominator was wrong in a second way: **82 checkouts
across 66 distinct repositories**, and under a per-repo ref the correct
denominator is 66. This seat restated her figure as "82 repos" without asking
what the unit was; that restatement is ours, not hers.

⚠️ **`claiming: N` is never coverage.** The sweep measures *claims*, not
*obligations* — it cannot distinguish a repo that is correctly unclaimed from
one that should claim the guard and does not, and no work on the sweep closes
that, because the information is in no repository. Rule M's second clause,
earned here: **a population is only meaningful if it is the set the reader
thinks it is.**

### The process note worth more than the near-miss

Two seats both worked from a thirty-five-minute-old artefact neither had
re-read, across a compaction. The recovery was not that either of us verified
harder — it was that the mistake was **chased immediately after dispatch and
said out loud**, and the loop closed in about four minutes with nothing landed.
Protect that property. A wrong instruction chased beats an uncertain one sat on.

---

## How iridium is consumed by another project — asked by Waffles, 8 Aug 2026

Tom has approved a manifold composition restyle whose composer rebuilds with
iridium as the editing surface. Waffles asked whether iridium needs publishing
to a registry. **The hard part of consuming iridium is not the TypeScript, it
is the wasm**, and the facts below were measured, not remembered.

### Four facts, three of which are traps

1. **There is no root `package.json`.** Iridium is not a JS workspace; an
   install at the repo root does nothing.
2. **Both packages ship TypeScript *source*, not build output.**
   `@iridium-editor/core` `0.1.1` and `@iridium-editor/syntax-worker` `0.1.0`
   have `exports` pointing at `./src/**/*.ts` and `files: ["src/"]`. Every
   consumer compiles them. Fine behind a bundler, useless to anything expecting
   JavaScript.
3. ⚠️ **The package names do not match the directory names.** The folder is
   `packages/@iridium/core`; the package is **`@iridium-editor/core`**. An
   import path inferred from the tree fails.
4. 🔴 **The wasm is not in the repository.** `pkg/` is gitignored
   (`.gitignore:34`), and `@iridium-editor/core` peer-depends on
   `iridium-bindings >= 0.1.0` — a package that exists **only as wasm-pack
   output**. So vendoring iridium means the consumer's build needs a **Rust
   toolchain**, not merely a TypeScript one.

### ⭐ The finding that governs version discipline either way

`iridium-bindings`' `package.json` is **machine-generated from `Cargo.toml`**,
whose version is `0.1.0` and is bumped by nothing. **Every build ever produced
is `0.1.0`.** Pinning `iridium-bindings@0.1.0` therefore pins *nothing* — two
builds a week apart are the same version string over different bytes.

This is Rule M in package-manager clothing: **a version that cannot vary is
indistinguishable from an absence of versioning**, and it is worse than no
version because it *looks* pinned. Fix it — bump the workspace version per
publish, or stamp the short sha into the wasm package version — **before**
either vendoring or publishing is relied on.

### The recommendation, and the question it turns on

**Does the consuming build environment have a Rust toolchain?**

- **Yes → vendor for this slice.** Pin iridium by **commit sha**, not version,
  and commit the wasm build as one scripted step. The toolchain pin (#74)
  makes that reproducible. Publishing now buys little while the API moves
  hourly.
- **No → publishing is mandatory**, and the artefact that matters is
  **`iridium-bindings`** (the binary), not the TS packages.

**If publishing: GitHub Packages under the `tomWhiting` scope.** The consuming
project is private so the auth is already GitHub-shaped and needs no new
secret; it avoids claiming a public npm name while the API changes daily; and
iridium is MIT over a public repo, so nothing is concealed either way — which
makes the choice **reversible**, a rename rather than a migration. Stay on
`0.x`, pin **exact** with no caret, since pre-1.0 minors are breaking by
convention.

### What was declined

Waffles' record said the tip was *"A1 through A4 plus the colour fix at frame
main `ea4a414`"*. **`ea4a414` is not a valid object in iridium**
(`git cat-file -t` → *"Not a valid object name"*), and neither the A-labels nor
"the colour fix" appears in iridium's history or this file. That is frame-side
naming for frame-side work. The confirmation was declined and the shas asked
for instead — **a confirmed-but-unchecked tip is how an integration ends up
built on something nobody pinned.** Iridium's actual tip at the time of asking
was `076dc790`.

---

## ⚠️ COMPACTION BATON — S-3 is UNCOMMITTED in the tree. Read this first.

### State right now

**Nothing of S-3 is committed.** The working tree carries the whole slice as
modified + untracked files under
`crates/iridium-editor/src/input/keyboard/` (`multi_char_pairs.rs`,
`multi_char_pair_tests.rs`, `multi_char_skip_tests.rs`,
`multi_char_backspace_tests.rs`, plus edits to `behaviors.rs`, `mod.rs`,
`edits.rs`, `behavior_tests.rs`, `scope_suppression_tests.rs`,
`actions/run.rs`). ⛔ **A `git show HEAD:<path> > <path>` on any of those
destroys hours of work. There is no undo.**

**Workflow `wf_613bf6d3-3e5` (task `wh75mdsby`) is RUNNING** — building B-13,
then an adversary and a gate runner. Its script is under the session's
`workflows/scripts/`. Read its `journal.jsonl` before believing any summary.

**Last measured battery, on the tree as it stood before B-13:**
**2,658 / 1,117 / 1,244 passing, 0 failed**, nine gates exit 0, plus
`cargo test -p iridium-lang --all-features` at 81. Use that as the baseline.

### What happened, in one paragraph

S-3 (multi-character auto-pair openers) was built by workflow across four
rounds. Insertion, longest-match and skip-over are sound and verified. **Step 5,
backspace, has been wrong four times.** Each round's fix was refuted by the next
round's adversary, and the rulings are all written up in
`docs/design/AUTO-PAIR-MAP.md` §10.8 through §10.12 — **read those, not this
summary.** The short version: B-8 bounded the delete's left side, B-11 tried to
bound the right with `not_in` at the caret, §10.11 corrected the probe to
before-the-opener, and round 4 showed that is a *regression* in five measured
cases. **B-13 (§10.12) abandons positional probing entirely** and gates the
collapse on remembered insertion — the record of what the editor wrote, matched
by cursor identity and revision, on the `preferred_columns` precedent.

⭐ **The lesson, if only one survives:** *"Did the editor write this closer"* is
a fact about **what happened**; the buffer records only **what is**. No position
in a post-edit buffer reconstructs a decision made in the pre-edit one.

### Do NOT redo these

- The nine-gate battery on `d1157a81` — done, green, recorded above.
- The theme research — `docs/design/THEME-SYSTEM-MAP.md` is committed (1,092
  lines, T-1..T-5). Tom has been told; no rush on the T-numbers.
- The iridium-consumption answer to Waffles — recorded above. He vendors at
  `aa0b6be5`, whose code tree is byte-identical to the battery-green tree.
- The figure sweep — 22 manifests / 8 declaring / 14 not, corrected in five
  places. `build.rs:12`'s "21 languages" is **deliberately** left: it is a
  query-tree claim and `languages.txt` lists 18 ids against 22 directories,
  which wants deciding, not swapping.

### Next actions, in order

1. **Read `wf_613bf6d3-3e5`'s result.** If the adversary is clean and the gates
   are green, **commit S-3** — it is the largest uncommitted thing in the tree
   and should not stay that way.
2. **#74** — write `rust-toolchain.toml` pinning **1.97.1**, with
   `components = ["rustfmt", "clippy"]` and
   `targets = ["wasm32-unknown-unknown"]`. Verified: 1.97.1 is already the
   active default here and `1.97.1-aarch64-apple-darwin` already has the wasm
   target installed, so this is a no-op behaviourally. MSRV stays 1.85.
3. **#88** — tier-1 groundwork as the first slice of Tom's ruled tier 2.
4. **#69** — the flaky GPU test.

⏸ **#89 (the git guard) is BLOCKED** — the claim predicate moved to a ref and
neither the marker nor the hook may be committed. See the task, not this file.

⚠️ **Do not start a new workflow while another is editing
`input/keyboard/`.** Two agents in that directory will corrupt each other.

---

## Post-compaction tick — #74's CI half landed, the toolchain file is held

`wf_613bf6d3-3e5` is **still running** (builder agent, transcript growing, ~10
minutes in at 20:15). S-3 remains uncommitted. Nothing in `input/keyboard/` was
touched this tick.

### ⚠️ Correction to "Next actions" item 2 above

It said "1.97.1 is already the active default here". That is wrong in the way
that matters. Measured:

```
rustup show            -> active toolchain: stable-aarch64-apple-darwin
rustc +stable --version                     -> rustc 1.97.1 (8bab26f4f 2026-07-14)
rustc +1.97.1-aarch64-apple-darwin --version -> rustc 1.97.1 (8bab26f4f 2026-07-14)
```

The **version** is identical; the **toolchain identity** is not. `stable` and
`1.97.1` are two separate rustup installations in two separate directories.
Cargo's rustc fingerprint keys on the compiler binary, not only on the version
string, so pinning switches identity and **the first build after the pin lands
is a full cold rebuild**. That is a one-time cost, not a symptom — but it is
also why the file must not land while a workflow is running cargo: it would
blow the running agent's incremental cache mid-gate and make its timings and
its verdict unreadable.

Rule M again, in toolchain clothing: *"already the active default" was a claim
about a version, and the population it needed to be about was an installation.*

### Verified, so the pin is not a leap

| claim | command | result |
| --- | --- | --- |
| `1.97.1-aarch64-apple-darwin` is installed | `rustup show` | listed |
| it has clippy and rustfmt | `rustup component list --toolchain 1.97.1-… --installed` | `clippy-…`, `rustfmt-…` both present |
| it has the wasm target | `rustup target list --toolchain 1.97.1-… --installed` | `aarch64-apple-darwin`, `wasm32-unknown-unknown` |
| no toolchain file exists anywhere above us | `ls` in repo root, `ablative/`, `Developer/`, `~` | none — nothing to override or be overridden by |
| nothing builds for a third target | grep `--target` across `*.sh *.toml *.yml *.json *.md` | only `wasm32-unknown-unknown`; wasm-pack's `--target web` is an output flag, same triple |

The pinned toolchain carries **two** targets where `stable` carries ten. That
is fine by the last row and not by assumption.

### Done this tick

- **`.github/workflows/ci.yml`** — all four `dtolnay/rust-toolchain@stable`
  steps replaced with `rustup show`, so the workflow **names no version
  anywhere**; the version lives in `rust-toolchain.toml` alone. `components:`
  and `targets:` inputs deleted from the three jobs that carried them — the
  toolchain file declares them once, which is what equips a developer's
  checkout as well as a runner. The two long "Record the toolchain" comments
  argued at length *from the premise that no pin exists*; both were rewritten
  rather than edited, because the pin falsifies their first sentence. The
  version print **stays**, with its reason changed: the pin says what was asked
  for, `rustc --version` says what was resolved, and they come apart if rustup
  is too old to honour the file or a runner sets `RUSTUP_TOOLCHAIN`.
- **Task #94 opened** — `.cargo/config.toml:19` calls `cargo ci` "what CI would
  run" while running three of the nine gates, and its `test --workspace` has
  neither `--all-features` nor `--no-fail-fast`. Same false-green shape as the
  committed-but-disabled workflow file, except this one executes, so it looks
  more like evidence rather than less.

### Held, deliberately

`rust-toolchain.toml` is **written but not in place** — it sits at
`…/scratchpad/rust-toolchain.toml`. It lands the moment the workflow is done,
together with the ci.yml change, **never before**: ci.yml's new comments point
at a file that must exist when they are read, and a commit of the CI half alone
would be a live reference to nothing.


---

## #89 CLOSED — the class-A guard is installed, claimed and PROVEN

Unblocked by the ref-predicate migration actually landing. The old blocker was
that `.shared-tree` could not be committed and the hook keyed on it. Measured
today: `tools/gates/hooks/claim_predicate.sh` names `refs/guards/shared-tree`
and nothing else, and `reference-transaction` **sources** that predicate from
`dirname $0` rather than hand-keeping its own copy — which is the exact defect
I flagged earlier, now fixed at the source. Blocker gone.

### The install, and why it has no pointer in it

    cp -p <gates>/hooks/claim_predicate.sh    .git/hooks/    # predicate FIRST
    cp -p <gates>/hooks/reference-transaction .git/hooks/
    chmod +x .git/hooks/reference-transaction

**Predicate first, deliberately.** The hook fails CLOSED when it cannot load
its predicate — it refuses *every* reference transaction, not just rewrites —
so the reverse order opens a window in which an ordinary commit is refused.

⛔ **`core.hooksPath` stays UNSET**, and this inverts what the detector used to
require. The default hooks directory is already repository-scoped; a *relative*
hooksPath is a working-tree property that only guards trees whose branch
carries it, and an *absolute* one is the stale-path class that silently
disarmed four estate repos when directories moved. Verified unset here.

`.githooks/reference-transaction` + `.githooks/claim_predicate.sh` committed as
the tracked canonical (`fe54c56b`), hook staged **100755** — a 100644 hook is
ignored by git silently. The detector compares the installed copies against the
**index**, and iridium is obligated, so an unverifiable identity is itself red.

### ⭐ Acceptance is a refused rewrite, not an install — and the probe was run red first

The negative control matters more than the pass. Same command, same repo, same
two commits; the **only** variable is the claim:

| state | `git update-ref refs/heads/guard-acceptance HEAD~5` | ref after |
| --- | --- | --- |
| unclaimed | **exit 0** — five commits discarded, no complaint | moved back |
| claimed | **exit 128**, refusal printed | **UNMOVED** |

Not bricked, proven two ways: the fast-forward move back to HEAD was allowed,
and commit `fe54c56b` itself landed under the armed guard.

**Non-fast-forward via `update-ref`, chosen over amend deliberately.** git
reports an all-zero `old` for `update-ref`, `branch -f`, `checkout -B` and
`rebase`, and a real one only for amend and reset. An acceptance test built on
amend alone samples the half of the class that happens to work — which is how
the hook's own first version shipped waving `rebase` through with a passing
test. It also needs no worktree and no checkout, both banned in this tree.

Detector, run at this seat:

    check_guard_active.sh --arming    -> exit 0, "class-A guard ARMED",
                                         hooksPath [UNSET], hook AND predicate
                                         byte-identical to the tracked pair
    check_guard_active.sh --artefacts -> exit 0, "the class-A guard SHIPS"
    check_must_be_claimed.sh          -> exit 0, 4 obligations, every one
                                         CLAIMED + ARMED

### ⚠️ Three stale statements I did NOT edit, because gates is not my tree

1. **`docs/tracking/must-be-claimed.tsv`'s header contradicts its own body.** It
   still carries a loud `⛔⛔ THIS FILE IS EMPTY OF OBLIGATIONS` block and says
   `libs/iridium ... is NOT written in below` — above four rows, the first of
   which is libs/iridium. A reader who reads the header block and stops
   concludes the estate has no obligations.
2. **Both instruments still state that nothing runs them.** `claim_predicate.sh`
   and `check_guard_active.sh` carry `MEASURED 2026-08-08: no battery invokes
   this script ... anywhere in the estate`. That was true when written and is
   false now.
3. ⭐ **The hook's refusal text is wrong at the worst possible moment.** It tells
   the reader `NOTHING WILL CHASE YOU FOR IT ... RIGHT NOW IT IS THE ONLY RECORD
   THERE WILL EVER BE` — printed exactly when someone is deciding whether to
   record an unclaim. It now argues, falsely, that the ledger edit is optional.

### ⚠️ And the leg's population is four, not six

Cally's note said the check runs as a leg in six repos (aion, beamr, frame,
haematite, liminal, meridian). Measured across every `gates.json` in the estate,
the leg is `check_must_be_claimed.sh` and it is present in **four**: aion,
frame, haematite, apps/meridian. **beamr and liminal carry no reference to
either guard instrument anywhere in their trees** — their legs are fmt, clippy
and tests. Two of the six would have shown green today by not running the check.

Rule M, third time this week: *a green must state its population.*


### ⚠️ CORRECTION to the section above — my own four-versus-six over-reached

Waffles re-measured (he had relayed someone else's number once already today
and declined to do it twice). It splits three ways and **nobody had all of it**:

| repo | state |
| --- | --- |
| aion, frame, haematite, apps/meridian | leg runs in the checked-out tree **today** |
| liminal | leg is on `main`; the checkout sits on `design-51-blob-store`, so it **inherits on rebase** |
| beamr | leg landed and was **deliberately reverted** at `48a12a5`, with a measured cause |

**"Four" was right for the question I asked and wrong for the sentence I
wrote.** I said beamr and liminal "carry no reference to either guard
instrument **anywhere in their trees**". I measured the **checked-out
worktree** and published a claim about the **repository**. Verified after the
correction: `git show main:gates.json` in liminal matches
`check_must_be_claimed` — it was there the whole time, one branch away from
where I looked.

⭐ **Rule M's second clause, turned on me: a population is only meaningful if
it is the set the reader thinks it is.** "In their trees" reads as *in the
repository*; a grep of a working directory answers *on this branch*. Same
species as the manifest-versus-resolved-graph error in the theme map — I asked
a cheap proxy and reported it as the target.

**The beamr revert is the finding neither Cally nor I had, and it is a good
one.** Artemis measured that beamr reads its legs out of `gates.json` **at run
time and evals them in a GitHub Actions checkout**. The estate leg carries
paths absolute to this box, which do not exist there, so it exited **127** and
the verdict step turned that into a red on every push. Beamr is the only one of
the six whose CI executes the legs. Cally reverted it herself and wrote the
cause into the commit, including that she had wired it believing a `gates.json`
is a hand-run landing battery — which is what the runbook says — without
measuring whether anything else consumes it.

So the honest sentence is: **the estate check runs in four repositories,
reaches a fifth on rebase, and is deliberately absent from the sixth because
the check asserts about a specific machine and cannot run in a CI checkout by
construction.**

### ⚖️ Waffles' ruling on the refusal text — stronger than what I proposed

I asked that the sentence name its population. **Ruled: strike the coverage
claim entirely.**

> A per-repository hook that asserts a fact about the whole estate is making a
> claim whose scope does not match the artifact making it — and worse, it is a
> fact that **expires**, embedded in an artifact that **travels** to other
> repositories and sits there for months.

True when written, false four hours later in four repos, still true in two. No
wording survives that. What stays is what is true everywhere and permanently:
unclaiming leaves no trace, and the sanctioned disarm is a ledger edit. Those
are properties of the mechanism. *"Nothing will chase you"* is a property of
this week's wiring and does not belong in a file that outlives it.

⭐ The general form, worth carrying into iridium's own comments: **a fact with
an expiry date must not be written into an artifact that travels.** This
codebase does exactly that in places — every `MEASURED <date>` block in a file
that ships. The ones that state a *mechanism* are fine; the ones that state a
*coverage* are the hazard.

My deviation (branch-ref probe instead of a scratch worktree) was **ratified
and goes into the runbook as the preferred shape**, not as an exception.


---

## S-3 and #74 LANDED — `47e91eb7` and `711400dc`, pushed

The nine-gate battery re-run **at this seat**, under the pinned toolchain, each
command separate with its exit status checked as its own statement:

| gate | exit |
| --- | --- |
| all eleven (fmt, 3 test, wasm check, 4 clippy, fmt --check, iridium-lang) | **0** |

**2,671 / 1,125 / 1,257 passing, 0 failed**, `iridium-lang` 81. Exactly +2 on
each against the workflow's 2,669 / 1,123 / 1,255, which reconciles to the two
ungated tests I added. Toolchain recorded in the run:
`1.97.1-aarch64-apple-darwin (overridden by rust-toolchain.toml)`.

### ⭐ The re-run was not ceremony — the agent's green was stale

The workflow's gate agent reported all nine exit 0, truthfully, **about the
tree it left**. My two added tests then put three clippy gates at **exit 101**
(`redundant_clone` ×3, all mine). Fixed and re-run to green.

*A gate result is a fact about a tree, not about a slice.* Anything that edits
the tree after the gate ran has invalidated it, including the person reading
the report. This is the concrete case for the standing rule that a reported
green is re-run at this seat before a commit.

### The adversary pass, and the two findings that mattered

Recorded in full in `AUTO-PAIR-MAP.md` **§11**. Both are now pinned and both
red-proved by their own one-line mutation, each failing exactly one test:

- **F1** — deleting the document-identity term from `describes` left the
  *entire* suite green, because `set_content` uses `continuing_from`, which
  bumps the revision, so that term is never the reason for a `false`. A term
  the examined set agreed with its own absence on — the same shape that refuted
  rounds 2–4, this time **inside the fix rather than the probe**. Pinned
  directly against the predicate (no keystroke can reach it), not deleted.
- **F2** — the motion round-trip revival was argued at length and tested
  nowhere. ⭐ **Refusal tests cannot catch a guard that became too eager.**
  Red-proved with the feared change itself: `self.auto_pair = None` in
  `reset_vertical_state`, where anyone would naturally group it with the sticky
  columns.

F3's four rotted doc links fixed. F4 was my own held ci.yml, now landed. F5 is
a perf note the adversary declines to call a defect; left as written, recorded
so the next reader knows it was seen.

### ⚠️ The debt this slice created, stated not buried

`behaviors.rs` **755 → 806** against an explicit do-not-grow instruction;
`mod.rs` **475 → 509** and `editing/mod.rs` **479 → 515** newly over the bar.
`mod.rs` crossing counts twice — the standing rule is that it carries
declarations only. The directory goes 9 → 13 files over 500. Three of the four
biggest additions are test files (expected mass for a feature this contested);
these three are not. **#92**, and it is mine.

### Queue after this

1. **#92** — the file-size debt, which this slice just made worse.
2. **#88** — tier-2 language extensions (Tom's ruling), unblocks #43→#44→#45.
3. **#69** — the flaky GPU test.
4. **#95** — the expiring-coverage-comment sweep, new law from the guard work.
5. **#94** — the `cargo ci` alias that runs three of the nine gates.


---

## #92 attempted and reverted — the obvious `mod.rs` split cannot pass the gate

Tried the debt S-3 created, starting with the clearest violation: `mod.rs` at
509 lines when the rule is declarations only. Moved `KeyboardHandler` — struct,
`Default`, impl, 380 of the 509 lines — into `handler.rs`. `mod.rs` came out at
115 lines. It compiles. **It cannot pass `clippy -D warnings`.**

**Step 1 was fine.** Siblings (`dispatch.rs`, `keymap_api.rs`, `edits.rs`) read
`KeyboardHandler`'s *private fields*, which only worked because the struct sat
in `mod.rs` and privacy extends to descendants. Twelve fields and
`create_selection_command` became `pub(super)`; nine call sites fixed; arguably
an improvement in its own right.

**Step 2 is the trap, and it is worth remembering.** `mod.rs` imports
`Document`, `Command`, `CursorState`, `Selection`, `CaretScopes`,
`EditorConfig`, `UndoTree` and four `commands::` names — and **ten files here
consume them through `use super::*`**. The moment `mod.rs` stops using them
itself, rustc calls every one an **unused import**.

⛔ **The warning is a false positive, and it is a convincing one.** Acting on
it — deleting the eleven imports it names — produced **625 errors across nine
modules**. `unused_imports` does not credit a descendant's glob or `super::X`
as a use.

It also cannot be silenced honestly: `pub(super) use` does **not** clear it
(measured: still 9 warnings, plus E0365 — a `pub(super)` item cannot be
re-exported `pub(super)`), and `#[allow]` is banned outright by CLAUDE.md.

⭐ **So the real task is not a file split.** It is that ten files reach through
`mod.rs` for names `mod.rs` merely happened to import — a hidden coupling that
made the split look cheap and is exactly why it wasn't. Replacing `use super::*`
with explicit `crate::…` imports is its own slice with its own battery.

**Tree returned byte-identical to `47e91eb7`** (`git diff --stat HEAD` empty).
Nothing of the attempt committed. The full finding, the measurements, and a
four-step order for the next attempt are on **#92**.

⚠️ **The lesson generalises past this directory:** *an "unused import" on a
module whose descendants glob it is a claim the compiler cannot check.* The
same shape as everything else this week — a cheap signal standing in for a fact
it does not actually observe. The difference is that here the signal is the
compiler's, which makes it far more likely to be obeyed without measurement.


---

# ⭐ COMPACTION BATON — 8 Aug 2026, ~21:5x

## Committed and pushed earlier this session
`47e91eb7` S-3 · `711400dc` toolchain pin · `fe54c56b` guard install ·
`3d2071a2`, `0440e387`, plus #92's finding. All pushed.

## ⚠️ UNPUSHED: the guard currency commit

The last commit (guard update to gates `a2fa336`) **is committed but I did NOT
get to `git push`.** Push it.

### What it was
Cally reported, and I **red-proved here before changing anything**, that the
guard I installed this morning had a live bypass:

```
git commit-tree HEAD^{tree} -p HEAD     # commit reachable from one throwaway branch
git update-ref refs/heads/<probe> <it>
git branch -D <probe>                   # -> exit 0, COMMIT ORPHANED
```

Install and tracked canonical were **both** `22e2dd4e` — stale *together*, so
the identity check passed and was right to. Integrity held; currency did not.

Updated to gates `a2fa336`. Hashes now: hook `5bc7d3ec`, predicate `fd5d0b6a`
(both match what Cally predicted). Install copied **before** the commit,
deliberately.

### Verified at this seat
- `check_must_be_claimed.sh` → iridium reads **`CLAIMED + ARMED + CURRENT`**.
  (The script's overall exit is 1 because **another** repo's row is red — NOT
  iridium. Confirm which before anyone reports iridium red.)
- Same `branch -D` probe under `a2fa336` → **exit 128, ref survived**, refusal
  names orphaning. Bypass closed.

### ⛔ TWO LOOSE ENDS
1. **`refs/heads/guard-orphan-probe2` still exists** — the probe branch, left
   because the guard now (correctly) refuses to delete it. Its commit is
   `HEAD^{tree}` with HEAD as parent, so it is harmless. Removing it needs a
   deliberate unclaim; **leaving it is the safer default.**
2. **`git gc --prune=never` timed out at my 2-minute limit** — NOT a failure,
   but NOT a pass either. ⚠️ The old hook's whole reason for leaving deletion
   out of scope was that a deletion rule breaks `gc`/`pack-refs`. `a2fa336`
   claims to separate prunes by *survival evidence* rather than intent. **That
   claim is UNVERIFIED here.** Run `git gc` with a long timeout and check the
   exit status before telling anyone gc is fine on this repo.

## #69 — in flight, experiment running

**A 50×-paired repetition job (`bhucp08b5`) may still be running.** It runs
`experiment_69_maximised_atlas_divergence` and the original test 50 times each
and prints `runs= experiment_failures= original_failures=`. Read
`…/scratchpad/` for `FAIL-exp-*` / `FAIL-orig-*` files.

⚗️ **There is an UNCOMMITTED temporary test appended to
`crates/iridium-editor/tests/retained_shaping/gutter.rs`
(`experiment_69_maximised_atlas_divergence`). It must be REVERTED, not
committed** — `git show HEAD:crates/iridium-editor/tests/retained_shaping/gutter.rs > …`
is safe, that file is committed.

### Two findings that outlive the experiment
1. ⭐ **The doc's own proposed experiment cannot discriminate.**
   `IN-FLIGHT-69-flaky-gutter.md` proposes warming the cold compositor so both
   atlases match. The test already passes 160/160, so making the two *more*
   alike cannot fail either way — a proxy that agrees with its target on the
   entire examined set. My experiment inverts it: **maximise** divergence (warm
   composes a `#` gutter first, so its atlas holds `#`, digits, then `+`, while
   cold sees only digits and `+`). That one *can* fail.
2. ⭐⭐ **The atlas hypothesis' edge-sampling half is DEAD.** Measured in
   glyphon 0.10 `src/cache.rs:50-52`: `min_filter`, `mag_filter` and
   `mipmap_filter` are **all `FilterMode::Nearest`**. With nearest sampling
   inside an allocated rect, a glyph's atlas *position* cannot bleed a
   neighbour's texel into the output. So "sampling picks up a neighbouring
   glyph's texel at an edge" is not an available mechanism, and the doc states
   it as one.
   ⛔ **CORRECTED 8 Aug 2026 — the finding above OVER-REACHED and must not be
   quoted as written.** The filters are as stated, so *blending* a neighbour in
   is genuinely impossible. But `shader.wgsl:110` computes
   `vert_output.uv = vec2<f32>(uv) / vec2<f32>(dim)` — a float derived from the
   glyph's atlas position **and the atlas size** — interpolates it, and
   Nearest-*rounds* it at `:119`/`:122`. **Selecting** the wrong texel is
   entirely available, and an atlas growth that changes `dim` rescales every UV
   at once. Not blending, but selection. See
   `docs/IN-FLIGHT-69-flaky-gutter.md` §3.
3. **A better-fitting candidate, not yet tested:** 1 pixel / 1 channel /
   magnitude 23 looks like an **antialiasing** difference, i.e. the glyph was
   *rasterised* differently — which points at a **subpixel-position bin**, not
   at atlas packing. The custom gutter changes the measured gutter width and
   hence the content column (`frame.rs:94-96`), so a float difference between
   the warm and cold width paths could flip a subpixel bin. Worth checking
   `frame_gutter_width` vs `gutter_width` in
   `render/compositor/gutter_column.rs` — the file's own doc says there are
   "two width answers".
4. **A discriminator already in the tree, unused by the doc:** three sibling
   tests use the identical warm/cold pattern and only this one is reported
   flaky. It is also the only one that **introduces a new glyph** (`+`); the
   fold and gutter-toggle tests only remove glyphs.

## Next actions, in order
1. **Push the guard commit.**
2. **Verify `git gc` exits 0** (loose end 2 above).
3. **Read `bhucp08b5`'s counts, then REVERT the temporary test in `gutter.rs`.**
4. Fold the #69 findings into `docs/IN-FLIGHT-69-flaky-gutter.md`.
5. Then #88 (tier-2 language extensions), #95, #94, #92-proper.


---

# 📌 TICK — guard re-synced, #69 folded, gc verified

## ✅ 1. The guard, second round — `f3038dd2`, row `CLAIMED + ARMED + CURRENT`

Cally reported that their previous install step named a **working-tree** path in
a repo they were mid-edit in, and that Hephaestus Bagel installed uncommitted,
unreviewed bytes into a live hook that way — caught only by hashing the result.

⭐ **Checked before touching anything, and it came back clean *with provenance*,
not just value.** The installed hook was `5bc7d3ec73746db05f436538f07a6343b6645050`,
**byte-identical to the committed blob at `a2fa336:hooks/reference-transaction`**.
So this seat installed reviewed bytes, and that is now demonstrated by object
identity rather than inferred from a copy exiting 0.

Re-synced to gates `41ac19a`: hook `3c71db65`, predicate `fd5d0b6a` (unchanged),
both written with `git show HEAD:`, install first, both verified by hash.

⭐ **Cally called the release "a comment-only correction in the guard". Verified
rather than accepted — and it is true only of the file this repo runs.**
`git diff --no-ext-diff a2fa336 41ac19a -- hooks/` shows `reference-transaction`
differing solely in the block explaining why HEAD is omitted from the
reachability set (Waffles' three-case correction: the old one-case reason is
false for a **detached** HEAD, which *is* a genuine independent anchor). The
**behavioural** fixes in `41ac19a` are in the **checker**, not the hook — a
dirty-canonical precheck that was structurally unreachable because
`$canonical_repo` is the hooks *directory* while `git diff` pathspecs are
cwd-relative and `git cat-file -e HEAD:<path>` is root-relative.

## ⛔ 2. A GREEN ROW WITNESSES THE VALUE, NEVER THE MODE — new, unreported upstream

Found while installing. `claim_predicate.sh` is tracked **100644** upstream; a
seat who copies "both files" and `chmod 755`s both — as this seat did on the
first pass — commits a **mode drift** into the tracked canonical. Reverted here
before the commit, so nothing landed.

⭐ **The checker cannot see it.** `check_must_be_claimed.sh:647-648` compares
`git rev-parse HEAD:<path>` on both sides — **blob ids**. A blob id encodes
content only; the mode lives in the tree entry. So the comparison is
**mode-blind**, and a repo tracking `reference-transaction` as `100644` reads
`CURRENT` while any install taken from its own tracked copy is **not
executable — and git silently does not run a non-executable hook.** A green row
over a guard that cannot fire, which is the exact class `41ac19a` was written
about, one level along.

⚠️ **Not measured here, and deliberately so:** proving it end-to-end means
temporarily disarming a live guard. It belongs in gates' own harness
(`test_must_be_claimed.py`, next to arm 21), in a scratch repo, not in anyone's
working checkout. Handed to Cally as arm 22.

## ✅ 3. `git gc` VERIFIED — the last open claim from the previous baton

The old hook's entire justification for leaving deletion out of scope was that a
deletion rule breaks `gc`/`pack-refs`; `a2fa336` claims to separate prunes by
*survival evidence* instead. **That claim was unverified — the run timed out at
a 2-minute limit and was reported as unverified rather than assumed.**

Measured now, `git gc --prune=never` with the guard armed:

| | before | after |
| --- | --- | --- |
| exit status | — | **0** |
| refs | 8 | **8** — none lost |
| loose objects | 810 | 0 |
| packed objects | 17,253 | 18,063 |
| packs | 3 | 2 |

It genuinely repacked and ran `pack-refs`, and the guard did not refuse a single
prune. ⚠️ **Declared, not cleaned:** a `tmp_pack_8Xyzlq` (~36 MB) left by the
*earlier killed* gc is still in `.git/objects/pack/` and git labels it
"garbage". Removing it is object-store surgery on someone else's box, so it is
reported rather than done.

## ✅ 4. #69 — `3194ae0a`, and the finding is about the record, not the renderer

Full detail in `docs/IN-FLIGHT-69-flaky-gutter.md`, rewritten. Headlines:

- ⛔⛔ **The signature this doc reasoned from was never measured.** *"1 pixel of
  393216, one channel, magnitude 23"* traces to a **hand-written worked example**
  of the message format, introduced by the words *"On failure it now says:"*.
  ⭐ **The population proves it:** every readback target in the suite is
  512 × 384 = **196,608** pixels, so no run of this test can print 393,216. The
  real capture this sitting opens `40 of 196608`. #69's actual report carries
  **no signature at all**.
- ⭐ **An illustration placed in a record is indistinguishable from a
  measurement once the surrounding prose is gone.** Marked at the source so it
  cannot be re-harvested. And: **a cited figure carries a population, and a
  population is checkable** — this was caught by arithmetic.
- ⛔ The doc's own proposed experiment **cannot discriminate** — it makes the two
  compositors *more* alike, and the test already passes. Deleted, not marked
  done.
- ⚗️ Its inverse **can** fail, and did **once in 90 runs**: 40 pixels, columns
  61..=62, rows 10..=29, magnitude 149 — a solid glyph-sized block.
  ⚠️ 1-of-50 vs 0-of-50 is p = 1.0, **one event, not a rate difference.**
- ✅ **The load caveat is CLOSED.** The previous sitting's 160 runs never
  exceeded load 5.6; these 100 ran at 1-minute load **48 → 112**. The shipped
  test failed **0 of 50** there. Not-reproduced now stands at **210 executions**.
- The temporary test is **reverted**, `gutter.rs` blob-identical to HEAD, its
  source preserved in the doc §7.

## Next actions, in order
1. **#88** — tier-2 language extensions (Tom ruled), unblocks #43 → #44 → #45.
2. **#95** — sweep expiring coverage claims out of shipping comments.
3. **#94** — the `cargo ci` alias, which runs three of the nine gates.
4. **#92 properly** — replace `use super::*` in ten files *first*, then split.
5. Backlog: #87 (needs Tom's T-numbers), #90, #91, #93, #70.

**Still Tom's:** #31's three taste decisions, the oil-buffer key rulings (#58),
#87's T-numbers.

---

# ⛔ CORRECTION — I NAMED A CONSEQUENCE I HAD NOT MEASURED. TWICE, IN ONE DAY.

Cally narrowed the mode finding I sent as arm 22, and the narrowing is right.

**What I claimed:** *"a repo tracking `reference-transaction` as `100644` reads
CURRENT while its install is non-executable, and git silently skips a
non-executable hook. **Green row, dead guard.**"*

**What is true:** the first clause. `check_must_be_claimed.sh:647-648` compared
`rev-parse HEAD:<path>` — blob ids — and a blob id is mode-blind. Fixed at gates
`9cb6da6`, which now compares the **tree entry** via `ls-tree`, mode and content
in one comparison. Arms 22 and 22b are mutant-verified: reverting to
`rev-parse HEAD:<path>` kills both and nothing else.

⛔ **What is NOT true: "dead guard".** Cally measured it — the **arming** path
already carries `[ -x "$hook" ] || fail unprotected`, so a non-executable
*installed* hook was **already red**. The state I described was not reachable
through the install. What was genuinely uncovered is the **TRACKED** mode:
`--artefacts` asserts it, the sweep only ever invokes `--arming`, so the tick
covered the deployment and silently not the artefact. **An ornamental green over
the tracked canonical, not a live dead guard.**

## ⭐ THE PATTERN, BECAUSE THIS IS THE SECOND ONE TODAY

Earlier this session, on #69: *"glyphon samples `Nearest` on all three filters,
so atlas position **cannot** bleed a neighbour's texel — the mechanism is
**unavailable**."* The filters were measured and real. The conclusion was not:
Nearest rules out **blending**, and `shader.wgsl:110` derives the UV from atlas
position *and size* before Nearest **rounds** it, so **selecting** a wrong texel
was available the whole time.

Both have one shape:

> ⭐⭐ **A BLIND SPOT IN ONE CHECK IS NOT A BLIND SPOT IN THE SYSTEM.**
> I measured one instrument, found it blind, and published the *system-level*
> consequence — without asking what else covers that path.

It is Rule H (*a call-graph grep answers "who calls this", not "who can reach
this"*) one level up, and it is the mirror of Rule L: when several independent
rules could produce the same observable **acceptance**, a claim that one is
missing must rule out the others.

⚠️ **The tell is grammatical and it is cheap to catch.** In both cases the
measured fact was a *mechanism* fact — "this comparison ignores mode", "this
sampler does not interpolate" — and the published claim was an *outcome* fact —
"the guard is dead", "the mechanism is unavailable". **Crossing from mechanism
to outcome is a second claim, and it needs its own evidence.** Say what was
measured, then state the outcome as a question until something answers it.

## ⭐ What Cally kept from the exchange, which is worth keeping here too

- **Provenance was unobservable to her and observable to me.** Board H4 is
  corrected from "luck" to demonstrated: `5bc7d3ec` byte-identical to
  `a2fa336:hooks/reference-transaction`. So *"provenance is unobservable"* was a
  claim about the **verifier's position**, not about the world — **the holder
  can sometimes answer what the verifier cannot.**
- ⛔ **Mode is not uniform, and an "all guard files are executable" rule would be
  wrong.** At `apps/meridian` HEAD the hook is `100755` and the predicate
  `100644` — correctly, because the predicate is **sourced, not run**. Ours
  matches. Anything asserting executability as a *class* would red a correct
  estate forever.

## 🔴 A neighbour red that is not ours, and the trap in it

Two checkouts red — `apps/meridian-tools-seat` and
`apps/meridian/.worktrees/decision-notify` — because their branches **predate
the canonical commit**, so `ls-tree HEAD -- ":/.githooks/"` returns nothing.

⭐ **The asymmetry to remember: the hook that RUNS is repo-scoped (`.git/hooks`
is shared by every worktree), while the artefact that IDENTIFIES it is a tracked
file, so branch-scoped.** Every checkout on a branch older than the canonical
commit is un-identifiable *by construction* — protected, but unprovable.

⚠️ **Directly actionable here:** this seat is the one most likely to cut a
branch from an old point and then see the red. It is not a fault to fix; it is a
consequence to expect.

**Our row, re-derived at Cally's hands:** `CLAIMED + ARMED + CURRENT`, hook
`3c71db65`, predicate `fd5d0b6a`, exit 0. The `gc --prune=never` verification is
recorded on H1 as what turned that claim from believed to measured.
