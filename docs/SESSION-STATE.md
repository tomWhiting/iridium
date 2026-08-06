# Session state — 6 Aug 2026, 13:1xZ

## ✅ WHERE THINGS ARE — READ THIS FIRST

**Tabs work in the desktop face.** Not visible yet — there is no strip —
but the behaviour is live and bound. `⌘⇧]` / `⌘⇧[` walk the strip, `⌘W`
closes with a confirmation over unsaved work, and **opening a file makes
a tab instead of overwriting what is in front of you.**

Everything below is committed and green on all six gates.

| commit | what |
|---|---|
| `fd42466` | the window's size reaches every tab (kernel bug) |
| `f75b6a9` | Desktop A — `DesktopApp` holds a `Workspace<DesktopDocument>` |
| `19527a9` | Desktop B — the five workspace commands, additive open |

**Installed**: `/Applications/iridium.app`, built 13:03, signature
verified on the copy that landed. Tom quit everything at 03:03Z so the
install could run; he has been told what is and is not in it.

---

## ⏭ IN FLIGHT: Desktop C — draw the tab strip (task #48)

### ✅ Half of it is done and pushed

**`FrameCompositor::set_top_inset` landed.** The kernel half of the
crux below is finished: one field, read by the painter, the hit test,
`cursor_anchor_y` and `max_scroll_y`. Five headless tests in
`crates/iridium-editor/tests/top_inset.rs`, gated on the `render`
feature in `Cargo.toml` (`[[test]] name = "top_inset"`). All six gates
green, pushed.

The load-bearing test is a **round trip**: `position_to_pixel(line) → y`,
click at `y + line_height/2`, assert `pixel_to_position` gives that line
back — over four lines × three scroll offsets. Beside it a control that
stops two wrongs agreeing (the inset must actually move line 0 down).

### ▶️ WHAT IS LEFT

1. **Desktop face calls it.** `app.rs` — when the strip is up, call
   `shell.compositor.set_top_inset(10.0 + strip_height)`; when it is
   not, leave it. Do this wherever the strip height becomes known
   (`resumed`, `resized`, `rescaled`).
2. **Draw the strip.** `apps/iridium-desktop/src/overlay.rs` paints
   **floating panels** (`PanelAnchor` is Top/Bottom/Point, all
   *floating* and centred). A strip is a full-width band flush to the
   top edge — it needs its own placement, not a `PanelAnchor`.
   The painter already draws rounded rects (`PanelRect` carries a
   radius), so the vocabulary is there. **Tom's rule: rounded corners,
   genuine arcs, never sharp.**
3. **Strip content from the workspace.** `Workspace::tabs()` gives the
   node ids in display order; `Node::label()` the title; `active()` the
   one in front. A dirty marker per tab needs the same `is_dirty` test
   `app.rs` does, but per document rather than per active tab.
4. **Clicking a tab activates it**, and clicking its close box closes
   it — through `close_active_tab` so the unsaved-work question still
   gets asked. `pointer_pressed` must consult the strip *before* the
   document, the way `dismiss_modal_panel` does.

### The crux (kernel half now solved — kept for the reasoning)

The document must start *below* the strip.

`apps/iridium-desktop/src/overlay.rs` paints **floating panels** — the
palette, the undo tree, search, the context menu. A tab strip is not that
shape: it is a full-width band at the top that the document must be
pushed down by. Painting it as a floating panel would cover the first two
lines of text.

`FrameCompositor` hardcodes `let padding = 10.0_f32;` in **four separate
places** — `compositor.rs:642` (compose), `:1709` (`cursor_anchor_y`),
`:1740` (`pixel_to_position`), `:1810` — plus a `padding` field on the
metrics struct at `:194` that the closures read as `m.padding`.

**The plan, and why:**

1. Add `FrameCompositor::top_inset: f32`, default `10.0`, with
   `set_top_inset`. **A separate field from `padding`, not a repurposing
   of it** — `padding` is also the *horizontal* inset
   (`content_offset_x = gutter_width + padding` at `:647`), and those are
   two concerns that merely share a number today.
2. Every Y-axis use becomes `top_inset`: `:734`, `:786`, `:844`, `:1364`,
   `:1380`, `:1422`, `:1438`, `:1542`, `:1568`, `:1615`, `:1719`,
   `:1747`, `:1763`, `:1824`. Add `top_inset` to the metrics struct
   beside `padding`.
3. `max_scroll_y` (`:1684`) must subtract the inset from the usable
   height, or the last line cannot be scrolled to.
4. The desktop face sets `top_inset = 10.0 + strip_height` when the strip
   is up, `10.0` when it is not.

**The test that earns it, and the reason to do it this way:** a click
must land on the line under the pointer *with the strip up*. An offset
applied in the painter but not in the hit test agrees with the truth
exactly at the top of the document and diverges by one row everywhere
else — the same proxy failure as everything else this week. Write that
test first, against a composited frame, before drawing anything.

The web face is unaffected: its `top_inset` stays 10 and its tab strip is
DOM.

### Then the strip itself

Tom's rule: **rounded corners, genuine arcs, never sharp.** The overlay
painter already draws rounded rects (`PanelRect` carries a radius), so
the vocabulary exists; the strip needs its own placement rather than
`PanelAnchor`, which only knows Top/Bottom/Point *floating*.

Order after that: **B (done) → C (strip) → the sidebar**, which does not
exist at all in the desktop face.

---

## ⚠️ TWO THINGS I GOT WRONG TODAY — both told to Tom

1. **I rebuilt a binary a live session was running from.** `cargo build
   --release` at 13:01 rewrote `target/release/iridium-desktop` while
   pid 89462 had been running it since 12:06. No damage — macOS keeps the
   running process on its old inode — but the check belongs *before* the
   build, not after. `pgrep -x iridium-desktop`, always, first.

2. **Never restore a file with `mv` from a backup.** `cp x backup` then
   `mv backup x` gives the file the *backup's* mtime, which is older than
   the artefact cargo built in between — so cargo sees "unchanged" and
   silently reuses the stale build. I spent four tool calls convinced
   correct code was broken. Use `cp backup x`, or `touch` after.

---

## Standing rules (unchanged, non-negotiable)

- **Never** `git stash`, `git checkout --`, `git restore`, `git reset`,
  `git clean`, `git worktree` in this checkout. Agent briefs must name
  each banned command individually.
- **Never** ports 3000, 3030, 8000, 8080. **VITE BAN** — run-once builds
  only, `frame` serves, never `npm run dev`.
- A measured figure leaves this seat **only if the command producing it
  is in the same message's tool output**. **Never pipe or compound a
  command whose exit status you intend to believe** — redirect to a file,
  then `echo exit=$?` as its own statement.
- `pgrep -x`, never `-f`. Never kill pids 99844 or 31161. Binary swaps
  are check-then-swap as **separate** commands; install with `mv`, not
  `cp`. The no-swap rule rides Tom's **live session as a process**, not a
  pid number.
- git is wired to difftastic — `git diff --no-ext-diff`.
- No `unwrap()` / `expect()` / `panic!` / `todo!()` / `unimplemented!()`
  / `dbg!()` outside `#[cfg(test)]`. All public items documented.
  **Red test first for every bug fix**, proven to fail against the
  unfixed code.
- Files under 500 lines; `mod.rs` carries declarations only.
- ⚠️ **Anything meant for Tom leaves through the Meridian `send` tool, or
  it did not happen.** Tom `dm:c9255b2a-5731-4d17-8124-e3bfa2224186`.
  This seat is "Doug".
- `du -sk` on the lane tree at lane open and lane close.

## The six gates

Run `cargo fmt --all` first. Then, each unpiped, each exit status
checked on its own:

```
cargo test --workspace --all-features
cargo test -p iridium-editor --no-default-features
cargo test -p iridium-editor --no-default-features --features syntax
cargo check -p iridium-bindings --no-default-features --features web --target wasm32-unknown-unknown
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo fmt --all --check
```

---

## Open questions and known gaps

- **Awaiting Tom**: should a new file open into the current group or
  always at the top level? Defaulting to the current group — one line to
  change (`open_file` in `app.rs`, the `parent` binding).
- **#49 `app.rs` is 3,010 lines** against a 500-line bar. Pre-existing
  and now worse. Seams: the mouse block, scroll-and-viewport, painting,
  and the ~1,000-line test module.
  `workspace/model.rs` is 578 (down from 734) and wants its
  attach/detach/subtree machinery moved to a `tree.rs`.
- **#39** kernel has no `clear_language` — an unknown-extension file
  inherits the previous one's highlighting. Now *less* bad: a new tab
  starts with no language at all, so this only bites `save_as`.
- **#42/#43/#44/#45** the whole web-face half of tabs, untouched.
- **#40** five `f64 as usize` casts on JS line numbers in `wasm.rs`.
- **#35** HiDPI font scale not re-applied across displays.
- **#31** light/dark theme switch.
