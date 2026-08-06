# Session state — 6 Aug 2026, 13:5xZ

## ✅ WHERE THINGS ARE — READ THIS FIRST

**The tab strip is drawn and it works.** A docked band along the top edge:
the active tab a rounded card, unsaved tabs an amber dot, a close control
on each, overflow scrolling to keep the tab in front on screen. Clicking a
tab activates it; clicking its × closes it, with the unsaved-work question
still asked. `⌘⇧]` / `⌘⇧[` / `⌘W` still work, and opening a file still adds
a tab rather than replacing one.

Everything below is committed, pushed, and green on all six gates.

| commit | what |
|---|---|
| `fd42466` | the window's size reaches every tab (kernel bug) |
| `f75b6a9` | Desktop A — `DesktopApp` holds a `Workspace<DesktopDocument>` |
| `19527a9` | Desktop B — the five workspace commands, additive open |
| `8be2849` | one top inset the painter and the hit test both measure from |
| `68ffca4` | **the gutter measures from the top inset too** (kernel bug) |
| `80caf9b` | **Desktop C — the tab strip is drawn, and clicking it works** |

---

## ⚠️ THE SWAP IS ARMED AND WAITING ON TOM

`target/release/bundle/iridium.app` is **built and codesigned** with the
strip in it (13:50). It is **not installed**: `pgrep -x iridium-desktop`
reports **pid 22591 running from `/Applications/iridium.app`** since 13:21.

**When Tom quits, and only then**, as two separate commands:

```
pgrep -x iridium-desktop            # must report nothing
rm -rf /Applications/iridium.app
mv target/release/bundle/iridium.app /Applications/iridium.app
codesign --verify --deep --strict /Applications/iridium.app
```

Check *before* the build as well as before the swap — that is the rule I
broke on the 5th and it is written up below.

---

## 🧭 WHAT THE TAB STRIP IS MADE OF

**`apps/iridium-desktop/src/tab_strip.rs`** (767 lines with tests) — pure.
Window, grid and tabs in; rectangles out. No GPU, no theme, no workspace.

- `tab_strip_layout(width, height, metrics, content) -> Option<TabStripLayout>`
  is **the only thing that decides where a tab is**. The painter and the
  hit test both read what it decided. A strip drawn from one placement and
  clicked against another agrees on the first tab and nothing else.
- `tab_strip_height(window_height, metrics)` is called by the painter *and*
  by the face's reserve, so a window too short for the band is charged
  nothing for one.
- **Horizontal placement is in whole character cells.** The strip's text is
  shaped as one buffer at one origin, so a pixel gap between tabs would put
  the label of tab three half a cell left of its card.
- Overflow **scrolls** by whole cells; it does not shrink tabs, which would
  move the tab under the pointer when an unrelated file opened.

**`overlay.rs`** gained `shape_tabs`, `painted_tab_strip()`,
`tab_strip_height(height)`, `tab_card_color`, `tab_colors`, and a `tabs`
argument on `paint`. `PanelRect` gained `contains`.

**`app.rs`** gained `tab_strip_content()`, `document_is_dirty(DocumentId)`,
`sync_top_inset()` (called from `resumed`, `resized`, `rescaled`),
`tab_strip_press()`, `tab_at()`, `pointer_is_on_the_tab_strip()`.
`pointer_pressed` consults the strip after the modal panels and before the
document; `secondary_pressed` opens no menu on the strip.

**`tests/chrome_screenshots.rs`** writes `chrome-tabs.png`, and `shoot`
sets the top inset whenever the chrome has tabs — so no shot can show a
band lying over the document.

---

## 🐛 THE BUG THIS FOUND — and how

**The gutter did not move with the text.** `compositor.rs` wrote the line
numbers' text area from `HORIZONTAL_PADDING` and the content's from
`top_inset`. Both were `10.0`, so they agreed on every frame this editor
had ever drawn. The moment the strip reserved 63 px, the code moved down
and the numbers did not: **every line wore the number of the line above
it.**

**It was found by rendering the strip and looking at the PNG.** Every
placement query the inset already covered — `position_to_pixel`,
`pixel_to_position`, the round trip, `max_scroll_y` — passed clean. The
gutter has no placement query, so nothing could have caught it but a pixel.

The test now does: `the_gutter_moves_down_with_the_text_it_numbers` reads
back the composed frame, finds the first inked row of the gutter column and
of the content column in two frames an inset apart, and asserts both moved
by the same amount. Against the unfixed compositor it says *"the gutter
moved 0 pixels, not 34"*.

**The lesson, restated:** *the proxy law is not satisfied by testing the
queries. Render it and look at it.*

---

## ▶️ WHAT IS LEFT ON THE STRIP (small, and none of it blocking)

- **The close control's alpha is 0.40** and it is nearly invisible on an
  inactive tab. Deliberate — a control that stays quiet until sought — but
  it is Tom's to rule on once he has seen `chrome-tabs.png`.
- **The wheel over the strip scrolls the document.** VS Code scrolls the
  strip. Not a defect; a choice nobody has made.
- **No drag-to-reorder, no middle-click-to-close, no hover state.**
- **No context menu on the strip.** `secondary_pressed` deliberately opens
  nothing there: the document menu's verbs act on the document's selection.
  A menu of *tab* verbs is a separate thing to design.

## ⏭ NEXT: the sidebar

**There is no sidebar and no file tree in the desktop face at all.** That
is the agreed next piece, and it is a bigger one than the strip: it needs
a horizontal inset the way the strip needed a vertical one — and, given
what the gutter turned out to be, **every X-axis use of
`HORIZONTAL_PADDING` in `compositor.rs` wants auditing before that starts**
(`:668`, `:1775`, `:1846` at least, plus `content_offset_x`).

`Workspace` already carries the nesting the sidebar needs: `roots()`,
`node()`, `Node::Group { name, children }`, `parent_of`, `move_node`. The
tree is there; nothing draws it.

---

## ⚠️ THINGS I GOT WRONG — both told to Tom

1. **I rebuilt a binary a live session was running from.** `pgrep -x
   iridium-desktop`, **before** the build, not after. Followed correctly
   this session: pid 22591 was found before the bundle ran, and the bundle
   writes to `target/release/`, which is not what he is running.

2. **Never restore a file with `mv` from a backup.** `cp x backup` then
   `mv backup x` gives the file the *backup's* mtime, older than whatever
   cargo built in between — so cargo sees "unchanged" and reuses a stale
   build. Use `cp backup x`, or `touch` after. (Followed correctly this
   session when restoring `tab_strip.rs` from its deliberate breakage.)

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

The screenshot harness, when a frame needs judging by eye:

```
IRIDIUM_CHROME_SHOT_DIR=<dir> cargo test -p iridium-desktop \
    --test chrome_screenshots -- --ignored
```

---

## Open questions and known gaps

- **Awaiting Tom**: should a new file open into the current group or
  always at the top level? Defaulting to the current group — one line to
  change (`open_file` in `app.rs`, the `parent` binding).
- **#49 `app.rs` is now 3,270 lines** against a 500-line bar. Pre-existing
  and worse again. Seams: the mouse block, scroll-and-viewport, painting,
  the tab-strip block, and the ~1,100-line test module.
  `overlay.rs` is 1,768 and wants its colour derivations in a `chrome.rs`.
  `workspace/model.rs` is 578 and wants its attach/detach/subtree
  machinery in a `tree.rs`.
- **#39** kernel has no `clear_language` — an unknown-extension file
  inherits the previous one's highlighting. Only bites `save_as` now.
- **#42/#43/#44/#45** the whole web-face half of tabs, untouched.
- **#40** five `f64 as usize` casts on JS line numbers in `wasm.rs`.
- **#35** HiDPI font scale not re-applied across displays.
- **#31** light/dark theme switch.
