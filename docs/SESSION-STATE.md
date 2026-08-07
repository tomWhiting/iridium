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
the custom-gutter frame must be byte-identical
  1 of 393216 pixels differ; the first at column 3, row 2; they span
  columns 3..=3 and rows 2..=2; the largest single channel difference is 23
```

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
