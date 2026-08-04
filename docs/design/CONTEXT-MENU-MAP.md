# Context Menu Map — right-click on the desktop face

Produced 4 Aug 2026 from a full read of the desktop mouse path, the
kernel's mouse machinery and command registry, the overlay chrome as it
stands after the rounded-panel reskin, and the vendored winit 0.30.13
sources. The ruling this map serves: the product owner ruled a
right-click context menu **IN scope** for the desktop face — not
deferred. This map prices the two honest ways to build one and proposes
the starter contents. Produced read-only; no build was run for it.

Citation legend: `A:` = `apps/iridium-desktop/src/app.rs` (1845 lines),
`M:` = `apps/iridium-desktop/src/mouse.rs`, `O:` =
`apps/iridium-desktop/src/overlay.rs` (1239 lines), `P:` =
`apps/iridium-desktop/src/command_palette.rs`, `cmd:` =
`apps/iridium-desktop/src/commands.rs`, `run:` =
`apps/iridium-desktop/src/run.rs`, `K:` =
`crates/iridium-editor/src/input/mouse.rs`, `C:` =
`crates/iridium-editor/src/editor/core.rs`, `R:` =
`crates/iridium-editor/src/render/rounded.rs`, `I:` =
`crates/iridium-editor/src/commands/builtin/ids.rs`, `H:` =
`crates/iridium-editor/src/commands/builtin/host.rs`, `meta:` =
`crates/iridium-editor/src/commands/meta.rs`, `cur:` =
`crates/iridium-editor/src/document/cursor.rs`, `mot:` =
`crates/iridium-editor/src/input/keyboard/motions.rs`, `L:` =
`Cargo.lock`, `w:` = the registry sources of
`~/.cargo/registry/src/…/winit-0.30.13/src/` (winit is **not**
vendored into the tree; the lockfile pins 0.30.13, L:3718-3719).

Prior art: `docs/DESKTOP-SHELL-PLAN.md` (D-B ruled mouse IN v1; the
latency measurements that define the face's identity),
`docs/design/DESKTOP-CHROME-MAP.md` (the rounded overlay chrome this
menu would ride; its R1-R7 rulings are all landed in the tree read
here).

## The three load-bearing facts, up front

1. **The right button already arrives and is dropped in exactly one
   place.** winit's macOS view forwards `rightMouseDown:` as a normal
   `MouseInput` with `MouseButton::Right` (w view.rs:595-607,
   w view.rs:1090-1098), the face's event match handles only
   `MouseButton::Left` (A:1348-1354, the guard at A:1349), and the
   kernel's handler ignores every non-left press by design
   (K:301-311, the `_ => MouseResult::Ignored` arm at K:311) — while
   its `MouseButton::Right` variant sits documented "Right mouse
   button (context menu)" (K:23-24), waiting. Receipts in §1.1.

2. **winit 0.30 has no context-menu API, verified in its sources.**
   The only menu-shaped method on `Window` is `show_window_menu` — the
   *Windows titlebar system menu*, documented "**macOS: Unsupported**"
   (w window.rs:1546-1563) and implemented on macOS as an empty no-op
   (`pub fn show_window_menu(&self, _position: Position) {}`,
   w platform_impl/macos/window_delegate.rs:1184). A native NSMenu
   therefore means a **new dependency** (muda is the realistic
   candidate) or hand-rolled objc2 AppKit code — either way a lockfile
   mutation, priced in §2.1.

3. **Everything a drawn menu needs except pixel anchoring already
   exists.** The overlay chrome draws genuine-arc rounded panels with
   shadow and hairline through the kernel's `RoundedQuadRenderer`
   (O:483, R:78-141), panel content is windowless testable data
   (`PanelContent`, O:268-277), and the palette shows the exact
   modal-list-that-runs-a-CommandId pattern a menu is (P:83-93,
   P:166-197). What no overlay does today: anchor at an arbitrary
   pixel (both anchors center horizontally, O:417-423) and react to
   the pointer at all (§1.3).

## 1. Verified ground

### 1.1 How mouse input flows today — and where Right dies

The winit event arm (A:1303-1365): `CursorMoved` updates the face's
tracked pointer position (A:1345-1347, stored at M:98-105 — winit's
`MouseInput` carries **no position**, w event.rs:278, so the tracked
position is the only anchor a click has). `MouseInput` matches
`MouseButton::Left` only (A:1348-1354): press → `pointer_pressed`
(A:824-845), release → `pointer_released` (A:848-857). A right press
falls through the `if` at A:1349 and vanishes — silently, today.

On macOS, winit's view receives `rightMouseDown:` and forwards it
through the same `mouse_click` path as the left button (w
view.rs:595-607); `buttonNumber() == 1` maps to `MouseButton::Right`
(w view.rs:1090-1098). Each right press is preceded by a
`mouse_motion` call (w view.rs:598), so the face's tracked position is
fresh when the press arrives — the anchor is honest. Note also:
**winit does not translate Ctrl+left-click into a Right event** (a
control-click reports button number 0 → `Left` with the ctrl
modifier); today the face maps ctrl *or* meta on a left press to the
kernel's add-cursor bit (M:191-195), so control-click currently means
add-cursor, not context menu. That collision is D-3.

Position→cell resolution is already split correctly for a menu: the
compositor's wrap- and fold-aware `pixel_to_position` resolves the
document cell (`hit_test`, A:861-877), and the kernel's `MouseHandler`
supplies the semantics through a synthesized grid (M:13-35). A menu
needs the *pixel* for placement and the *cell* for caret/word
semantics; both are one call away at the press site.

The kernel side: `handle_mouse` dispatches Press/Release/Drag only for
`Some(MouseButton::Left)` plus Scroll (K:301-310); everything else is
`MouseResult::Ignored` (K:311). Nothing kernel-side needs to change
for a v1 menu — a right press is a *face* concern (open UI at a
pixel), exactly like the palette. If the owner later wants right-press
semantics in the kernel (e.g. "right-click outside the selection
moves the caret" as a reversible command), `MouseButton::Right`
(K:24) and `MouseResult` (K:151) are the prepared seam.

### 1.2 The command dispatch a menu must reuse

The palette's dispatch is the pattern, verbatim (A:491-511):
`editor.run_command(id, CommandArgs::NONE)` first — the kernel's
palette/macro/AI entry point, applying document changes through the
same command-sourced, undoable path as a keypress (C:583-601) — and on
`CommandRunError::Unimplemented`, fall back to the face's
`dispatch_host_command` (A:586-608: `file.save`, `file.saveForce`
(cmd:61-63), `palette.open` (H:40), `history.togglePanel` (H:54)).
The result is consumed by the same `consume` that acts on
`EditorKeyResult::Clipboard` and `::Search` (A:550-566), so a menu
"Paste" goes kernel → `ClipboardOperation::Paste` → the face's
arboard path (A:690-710) with zero new plumbing. The MRU recency list
records dispatched commands (A:494, A:501); a menu run should record
there too, so menu use and palette use train the same ranking.

Menu labels and shortcut hints come from where the palette gets them:
`editor.commands()` is the registry (C:544), each `CommandMeta`
carries id, human title, category and a `mutates_document` advisory
(meta:53-72), and `editor.key_hints()` gives the bound chord per
command (C:629-631) rendered mac-style via `KeyLabelStyle::MacGlyphs`
(P:356). A menu row is a poorer palette row: title left, hint right.

### 1.3 The overlay system — what exists, what's missing

What exists (all landed, post-reskin): a second render pass on the
composed frame with `LoadOp::Load` (O:684-704), all chrome as one
ordered `Vec<RoundedQuad>` instance list (O:621) drawn before the text
(O:701-702); per-panel chrome pushed in order backdrop → shadow →
hairline → background → selected-row band → caret (O:811-871);
`RoundedQuad::new`/`::shadow` with genuine SDF arcs, radius clamped to
half the short side (R:78-141); `panel_geometry` as a pure,
GPU-free-testable placement function (O:397-442) with the chrome
constants at O:93-155 (radius 8, PAD 16/12, shadow 0/16/48 @ 0.55,
hairline 1 physical px); panel content as colourless
`PanelContent`/`PanelRow`/`Span` data (O:268-277) budgeted by
`LineBuilder`; painting driven from `redraw` via `panel_contents`
(A:1118-1198, A:1205-1227), later panels on top.

What is missing, precisely:

- **Pixel anchoring.** `PanelAnchor` is `Top | Bottom` (O:255-263);
  both center horizontally (O:417) and derive y from the window edge
  (O:418-423). No overlay anchors to an arbitrary point, and no
  clamping-at-edges logic exists (centered panels can't overflow).
- **Any pointer awareness.** No overlay is mouse-interactive.
  `pointer_pressed` gates only on an open *prompt* (A:824-827);
  the palette, search and history panels are keyboard-only, and — a
  pre-existing gap worth naming — **a click while the palette or
  history panel is open falls through to the document underneath**,
  moving the caret while the modal panel stays up. There is no
  hit-test against panel rectangles anywhere in the face. A context
  menu is the first overlay that *must* hit-test itself (row hover,
  row click, outside-click dismissal), and the machinery it adds is
  exactly what would fix the fall-through gap (D-4).
- **A "menu" content shape.** `PanelContent` assumes a leading field
  row for search/palette; a menu is rows-only with a disabled state
  per row. `PanelRow` has `selected` (O:230) but no disabled flag.

### 1.4 Selection, caret and clipboard facts a menu reads

- **Is there a selection:** `editor.state()` (C:431) →
  `.cursor.primary` (cur:145-147) → `Selection::is_collapsed`
  (cur:56). Multi-cursor state is the same struct.
- **But note the kernel's clipboard semantics before greying
  anything:** `clipboard.copy`/`.cut` copy *the caret lines* when
  nothing is selected (I:165-168, "Copy the selections, or the caret
  lines when nothing is selected") — so Cut/Copy are never truly
  inoperative. Greying them on a collapsed selection (the macOS
  convention) would misstate the kernel. D-5.
- **Read-only:** `editor.state().read_only` (read at A:617) — the
  honest disable bit for every `mutates_document` item (meta:64).
- **Word under cursor:** kernel-owned — double-click word selection
  (K:569) and the public `select_word_at` (mot:299). The transforms
  already operate on "selection, or the word under it" (I:107-116),
  so a menu need not compute words itself.
- **Clipboard availability:** arboard, lazily opened and kept
  (A:717-729); an empty pasteboard is a distinguishable
  `ContentNotAvailable` notice (A:745-756). Probing it at menu-open
  time to grey "Paste" is one `get_text` round trip. arboard is
  already in this crate's graph (L:111-123).

### 1.5 macOS native-menu facts

- winit 0.30.13 (L:3718-3719) exposes **no context-menu API**: the
  fact-check in the sources is §fact 2 above (w window.rs:1546-1563;
  the macOS no-op at w window_delegate.rs:1184). The only macOS menu
  surface winit has is `with_default_menu(bool)` — create or skip the
  default *menubar* at startup (w platform/macos.rs:409-448). Nothing
  for popups.
- The realistic dependency is **muda** (the tauri project's menu
  crate: NSMenu on macOS, incl. `popup` at a position on a window via
  raw-window-handle). It is nowhere in the estate today (no lockfile
  match). Its macOS backend rides the objc2 family; the lockfile
  already carries **two** objc2 generations — 0.5.2 under winit and
  0.6.4 under arboard/wgpu (L:119-123, L:1661-1676) — so muda would
  likely share the 0.6 family rather than add a third, **but its
  exact transitive set (and on Linux its gtk subtree) is
  unverified from here** — this lane runs no cargo, and resolving it
  is the first task of path (A) if chosen. Any choice of (A) mutates
  `Cargo.lock`: a graph mutation under this project's build economics,
  to be priced as such, not smuggled.
- The alternative to muda is hand-rolled objc2-app-kit
  (`NSMenu`/`NSMenuItem` + `popUpMenuPositioningItem:`): no new
  top-level concept in the graph (objc2-app-kit 0.3.2 is already
  present transitively, L:119-123) but it would become a *direct*
  dependency of the desktop crate, plus target-conditional unsafe
  AppKit code this tree currently has none of.
- The runloop hazard is structural, not hypothetical: NSMenu tracking
  runs a **nested runloop** in event-tracking mode for the life of the
  popup. This face's loop is `ControlFlow::Wait` (run:72) and frames
  are composed only on `RedrawRequested` (A:1118); during tracking no
  such event is delivered, so the editor freezes under the menu
  (cosmetically fine — nothing is changing) and, more sharply, the
  chosen item's event arrives via muda's channel *outside* the winit
  event stream, needing an `EventLoopProxy` wake and a user-event arm
  the app does not have. Command dispatch re-entering `DesktopApp`
  from inside the tracking loop is the reentrancy bug class to design
  against.

## 2. The two designs, priced honestly

### 2.1 Path (A) — native NSMenu

Build the menu from the registry at right-press, `popup` it at the
click point, dispatch the chosen `CommandId` through §1.2's path.

What it buys: the exact system look and feel, free dismiss semantics,
free keyboard navigation and type-select, free submenus, and the
system's own service rows if wired (Look Up, Translate, Services,
dictation) — the things a drawn menu can never have.

What it costs:

- **A dependency** (muda, or direct objc2-app-kit): the lockfile
  mutation of §1.5, plus tracking a crate whose release cadence is
  coupled to tauri's needs, for one face's one widget.
- **Event-loop integration**: an `EventLoopProxy` + user-event arm in
  `run.rs`/`app.rs` to receive menu selections; care that dispatch
  happens after tracking ends, on the winit thread, never reentrantly
  (§1.5). This is the risk concentration — it is the only place in
  the face where control flow would leave winit's model.
- **Enabled-state and hint duplication**: NSMenu items need their
  titles, key equivalents and enabled bits pushed *into* AppKit at
  open time — a translation layer from `CommandMeta`/`key_hints`
  (§1.2) into a foreign retained object graph, rebuilt per popup.
- **Not portable**: teaches the kernel and the other faces nothing;
  the web face has DOM menus, the TUI would need a drawn one anyway.
  Against one-kernel-N-faces, this is a face-private cul-de-sac.
- **Identity**: the menu would be the only piece of chrome on this
  face not drawn by the face — it will match macOS, and not match the
  palette an inch above it.

Files and size: `apps/iridium-desktop/Cargo.toml` + root
`[workspace.dependencies]` (+1 dep); `run.rs` (proxy plumbing, ~20);
`app.rs` (right-press arm, menu build/popup, user-event dispatch,
~120-180); no kernel changes; no overlay changes. Plus the unpriced
tail: lockfile churn and the integration-bug surface with the nested
runloop. Testing is thin by nature — AppKit popups don't run
headless; the state machine around them is what can be tested.

### 2.2 Path (B) — drawn overlay menu (recommended)

Another panel on the machinery of §1.3: pixel-anchored, painted with
the same `RoundedQuad` chrome (shadow, hairline, 8px arcs — the
standing rule, genuine arcs, never sharp, is inherited free), rows
from the registry, dispatch through §1.2 unchanged.

What it buys: zero new dependencies (zero lockfile motion); the same
look as the just-landed palette chrome, same theme system, same
fonts; fully portable pattern (the geometry function and the state
machine are face-code today, kernel-liftable the day a second GPU
face wants them); every behavior CPU-testable like the palette's
(P's test module is the template); latency identity preserved — a
menu frame is a compose cache hit plus a dozen rounded instances and
one small shaped buffer, the overlay pass's existing budget class.

What it must implement — the table stakes, enumerated:

1. **Placement**: anchored at the click pixel; clamped at window
   edges (flip vertically above the point when the bottom would
   clip; slide horizontally, never off-screen; degenerate windows
   decline to draw, the O:406-415 rule). New pure function beside
   `panel_geometry`, same testability.
2. **Dismiss**: outside click (swallowed, not passed to the
   document), `Escape`, focus loss (`WindowEvent::Focused(false)`,
   A:1357-1362 already has the arm), and any command run.
3. **Keyboard nav**: Up/Down with wrap, Home/End, Enter runs the
   highlighted item, modal swallow of the rest — the palette's exact
   key discipline minus the query field (P:166-197).
4. **Pointer nav**: hover highlights a row (the face's first use of
   `CursorMoved` against an overlay rect), click runs it; press
   inside the menu but on no row does nothing.
5. **Disabled rendering**: rows greyed via the theme's
   `line_number`-class colour (the palette's dim text convention,
   P:348-350) and skipped by nav/Enter; disabled = `mutates_document`
   ∧ `read_only`, plus Paste on a text-empty pasteboard (§1.4).
6. **Separators**: a row kind painted as a hairline (the O:139-143
   derivation), skipped by nav.
7. **Right-press caret semantics**: right-click inside the current
   selection leaves it; outside, moves the caret to the clicked cell
   through the existing press path so the menu's verbs act where the
   user pointed (macOS convention; D-6 rules it).
8. **No submenus in v1** — the starter set (§3) doesn't need them;
   flat with separators. A submenu is a second anchored panel and can
   be priced when a verb set demands it.

What it costs: those behaviors are *implemented, not inherited* — the
dismiss/nav/disable state machine is ~the palette's complexity class
without the search ranking. And it will never have Services/Look
Up/dictation; if the owner wants "Look Up" specifically, that is
path (A) or nothing.

Files and size:

- `apps/iridium-desktop/src/context_menu.rs` (new): item model
  (id/title/hint/enabled/separator), builder from
  registry+key_hints+editor state, key handling, outcome enum
  mirroring `PaletteOutcome` (P:83-93), tests. ~350-450 lines, over
  half tests if the palette's ratio holds.
- `overlay.rs`: `menu_geometry` pure function (anchor + clamp + flip)
  and a rows-only paint path reusing the existing chrome pushes; a
  `disabled` bit on `PanelRow` or a parallel `MenuRow`. ~120-180
  lines incl. tests. No changes to `panel_geometry` or existing
  builders.
- `app.rs`: `MouseButton::Right` arm (A:1348-1354), menu-open state +
  modal ordering in `press` (the A:387-399 chain gains one link at
  the front), outside-click interception in `pointer_pressed`/
  `pointer_moved`, dispatch via `run_palette_command` (rename or
  reuse), paint via `panel_contents`. ~80-120 lines.
- Kernel: **nothing required.** (Optional later: lift the geometry
  function kernel-side next to `rounded.rs` when a second face wants
  it.)

Risks: modest and local — the outside-click/modal interplay with the
existing panels is the one place to think (a right-click while the
palette is open: recommend it is swallowed, palette stays, no menu —
modal means modal); everything else is pure functions and a state
machine with the palette as prior art.

## 3. The starter verb set

Buildable today, entirely from registered commands dispatching through
§1.2 — no new command ids needed. Contents are explicitly a taste
surface the owner will tune (D-2); this is the buildable floor:

| Item | Command id | Notes |
|---|---|---|
| Cut | `clipboard.cut` (I:168) | kernel → `EditorKeyResult::Clipboard` → arboard (A:553-555, A:690-710); line-cut when collapsed (I:167) |
| Copy | `clipboard.copy` (I:166) | same path; line-copy when collapsed |
| Paste | `clipboard.paste` (I:170) | greyed when pasteboard holds no text (§1.4), if D-5 rules greying in |
| — | | |
| Select All | `selection.selectAll` (I:69) | |
| Select Word/Node | `ast.selectNode` (I:215) | context-sensitive: snaps to the smallest syntax node — the "select this thing I clicked" verb; degrades honestly without a grammar |
| Expand Selection | `ast.expandSelection` (I:217) | with `ast.shrinkSelection` (I:219) if the owner wants the pair |
| — | | |
| Toggle Comment | `comment.toggleLine` (I:159) | context-sensitive; grammar-aware |
| Duplicate Line | `lines.duplicateDown` (I:150) | |
| Delete Line | `lines.delete` (I:152) | |
| — | | |
| Undo / Redo | `history.undo` / `history.redo` (I:175-177) | greyed by `mutates_document` ∧ `read_only` only; history depth queries are available if the owner wants true greying |

Worth naming: **"Toggle Fold" has no registry id today** — folding is
`Editor` methods (`toggle_fold_at`, C:1300) and gutter hit-testing,
not a command (no `fold` match anywhere in I or H). A fold item means
registering kernel fold commands first — real but separate work, and
the honest reason it is absent from the floor set. Transform-case
verbs (I:107-139) are palette material, not menu material, until
submenus exist.

## 4. Proof plan

**Stays green:** every existing overlay, palette, search, history and
prompt test (the menu touches none of their builders); the mouse
round-trip suite (M:246-416) — the left-button path is not edited;
the kernel suites (no kernel change).

**New tests (all CPU-side, no GPU, path B):**

1. `menu_geometry`: anchor at point; clamp at all four edges; the
   vertical flip; radius > 0 in both presets (the standing-rule test,
   restated the DESKTOP-CHROME-MAP §5 way); degenerate windows
   decline.
2. The menu state machine: open on right press, item order from the
   registry, disabled items skipped by nav and refused by Enter,
   Escape/outside-click/run all close, modal swallow.
3. Dispatch: a menu-chosen `clipboard.copy` produces the same
   `ClipboardOperation` a keypress does; MRU records the run; an
   unimplemented id surfaces the A:505-507 message rather than
   silence.
4. Interplay: right-click with palette open is swallowed; menu open
   blocks document clicks; `Focused(false)` closes it.

**Judged by eye:** row height/padding against the palette's, hover
colour, the exact greyed alpha. Ends with a bundled rebuild handed to
the owner — the feel gate — same as the chrome reskin. (Path A's
proof plan would be thinner by nature: the popup itself cannot run
headless; only the translation layer tests.)

## 5. Decisions for the owner

- **D-1 — Native NSMenu vs drawn overlay.** **Recommend (B), the
  drawn overlay.** Grounds: (i) one-kernel-N-faces — (B) is a
  reusable pattern over renderers every face inherits, (A) is a
  macOS-only cul-de-sac; (ii) build economics — (B) moves the
  lockfile zero, (A) adds a dependency graph for one widget; (iii)
  the latency identity — (B) stays inside the measured
  compose/overlay budget and the winit event model, (A) hands control
  to a nested AppKit runloop the face was deliberately built without;
  (iv) the chrome identity just landed — the menu will look like the
  palette, arcs and shadow and hairline, not like a foreign import.
  The honest price: no Services/Look Up/dictation, and the dismiss/
  nav behaviors are built, not inherited (§2.2's enumerated table
  stakes). If system-services integration is a requirement rather
  than a nicety, that alone flips this to (A) — nothing else does.
- **D-2 — The verb set.** §3 is the buildable floor; every id cited
  is registered today. Owner tunes at the feel gate; note "Toggle
  Fold" needs kernel command ids first (§3) — in or out of this
  slice?
- **D-3 — Ctrl+click.** macOS convention says context menu; this face
  currently folds ctrl into the add-cursor bit alongside ⌘
  (M:191-195), so control-click adds a cursor today. Recommend:
  **right button only opens the menu in v1**, ⌘-click stays
  add-cursor, and ctrl-click's meaning is the owner's ruling — moving
  it to the menu matches the platform but changes shipped behavior.
- **D-4 — The click-through gap.** Clicks currently pass through the
  open palette/history panels to the document (A:824-827, §1.3). The
  menu's outside-click machinery makes fixing this nearly free.
  Recommend fixing it in the same slice (a click outside a modal
  panel dismisses it, macOS-style) — but it is a behavior change to
  shipped panels, so it is named here rather than assumed.
- **D-5 — Greying Cut/Copy.** macOS greys them without a selection;
  this kernel's copy/cut honestly operate on the caret line when
  collapsed (I:165-168). Recommend **keeping them enabled** — greying
  would misstate what the commands do — accepting the small departure
  from platform convention. Pure taste; owner rules.
- **D-6 — Right-press caret semantics.** Recommend the macOS rule:
  inside the selection, leave it; outside, move the caret to the
  clicked cell (through the existing resolved-cell press path) so the
  menu's verbs act where the user pointed. Cheap either way; stated
  so it is chosen, not stumbled into.

**Implementation file-touch list (path B, the whole change):**
`apps/iridium-desktop/src/context_menu.rs` (new),
`apps/iridium-desktop/src/overlay.rs` (menu geometry + rows-only
paint), `apps/iridium-desktop/src/app.rs` (right-press arm, modal
ordering, dismissal, dispatch). Kernel: untouched.

---

## RULINGS — 4 Aug 2026, all six ruled by Tom (his DM ~03:45Z:
## "let's go with all your recommendations in")

- **D-1: RULED (B), the drawn overlay.** The flip condition (system
  Services/Look Up/dictation as a requirement) was put to him
  explicitly and not taken.
- **D-2: RULED — the §3 buildable floor** (Cut, Copy, Paste, Select
  All, plus a Command Palette… entry). Contents remain his tuning
  surface at the feel gate. "Toggle Fold" stays out of this slice;
  fold command ids in the registry are noted as worth adding
  independently (palette-searchable folds), on the follow-up ledger.
- **D-3: RULED — right button only opens the menu in v1.** Ctrl+click
  keeps its shipped add-cursor meaning; ⌘-click unchanged.
- **D-4: RULED — the click-through gap is fixed in this slice.** A
  click outside an open modal panel (palette / undo tree) dismisses
  the panel and does not reach the document, macOS-style. This is the
  ruled behavior change to shipped panels.
- **D-5: RULED — Cut/Copy stay enabled with a collapsed selection**
  (they honestly operate on the caret line; greying would misstate
  them).
- **D-6: RULED — the macOS caret rule.** Right-press inside the
  selection leaves it; outside, the caret moves to the clicked cell
  through the existing resolved-cell press path, so the menu's verbs
  act where the user pointed.
