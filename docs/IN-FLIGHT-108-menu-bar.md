# #108 — the menu bar

**Status: ground verified, nothing built.** Written 13 Aug 2026, while #113 is
blocked on Tom's grammar ruling.

This task existed as one **explicitly unchecked paragraph** at the foot of
`docs/OPEN-DIALOG-GROUND.md`, which ended *"Nothing verified about it yet —
treat this paragraph as unchecked."* This file is that verification. Its central
claim turns out to be **wrong**.

---

## Ground

### G-1 — ⚠️ "winit does not do menus" is FALSE

The unchecked note said winit does not do menus, so the choice was `muda` (a new
dependency) or `NSMenu` by hand. **winit already builds an `NSMenu` menubar and
installs it**, unless a host calls `with_default_menu(false)`:

| | |
| --- | --- |
| the switch | `winit::platform::macos::EventLoopBuilderExtMacOS::with_default_menu` — `platform/macos.rs:429` |
| **default is on** | `app_state.rs:139` — `if self.ivars().default_menu { menu::initialize(&app) }` |
| what it builds | `platform_impl/macos/menu.rs` |
| this app | `run.rs:98` calls plain `EventLoop::new()`, so **the default menu is installed today** |

**And it contains exactly one menu — the application menu.** Read off
`menu.rs`: *About …*, *Services*, *Hide …*, *Hide Others*, *Show All*, *Quit …*
Nothing else. There is **no File, Edit, View or Go menu**, which is precisely
why the way in Tom asked for is missing.

⭐ **So #108 is not "build a menubar". It is "add menus to the menubar that is
already on screen".** That is a much smaller task than the note priced, and
getting it wrong in the other direction — ripping out winit's app menu to build
a replacement — would lose *Quit* and *Hide* for nothing.

### G-2 — No new crate is needed, and this is now measured twice

`objc2-app-kit 0.3.2` is a **direct dependency** of `iridium-desktop`
(`Cargo.toml:96`), already used for `NSOpenPanel` in `dialog.rs`. `muda` does
**not** appear in `Cargo.lock` at any version. `NSMenu` and `NSMenuItem` are
additional **feature flags** on a crate already compiled in — not a new edge,
which is even cheaper than the open-panel decision was.

The existing feature list is `NSApplication`, `NSOpenPanel`, `NSPanel`,
`NSResponder`, `NSSavePanel`, `NSWindow`. `NSMenu` and `NSMenuItem` are the
additions; the block's own comment explains why `default-features = false` and
each flag is named, and that reasoning carries over unchanged.

### G-3 — A menu item that runs *our* command needs an Objective-C target

winit's items use stock AppKit selectors (`terminate:`,
`orderFrontStandardAboutPanel:`) with a nil target, so they travel the responder
chain to `NSApplication`. **Nothing in that chain knows what `file.open` is.**

`objc2 0.6.3` ships `define_class!` (`src/macros/define_class.rs`), so declaring
one small Objective-C class with one action method is available and is the
route. The item constructor winit uses is
`NSMenuItem::initWithTitle_action_keyEquivalent`.

⛔ **The cheap alternative is a trap and is ruled out here so nobody rediscovers
it.** Items could be added with a key equivalent, no action and no target —
they would *display* the chord and be permanently greyed. That is furniture
making a promise the routing does not keep, the same failure #112d step 6
refused for the sidebar. A menu item that cannot be clicked is not a way in for
"the hand that does not know the chord", which is the whole point of the task.

### G-4 — The precedent for a macOS-only feature is already set, and it is honest

`dialog.rs` is the model: `cfg(target_os = "macos")` dependencies, and a
three-outcome `Chosen` enum whose module doc says *"There is no chooser, and
`choose` says so rather than pretending… inventing a stub for it would be the
same lie `Unsupported` exists to avoid."*

A menu bar differs in one way worth noting: an absent menu on Linux is not
something the user can press and get silence from, so it needs no `Unsupported`
channel — it is simply not built. The `cfg` gate is the whole story.

### G-5 — Where the titles and the chords come from

`CommandRegistry` holds every command's title and category; `KeyHintIndex` holds
the chord that runs it today, with a `MacGlyphs` label style that already
renders `ctrl` as `⌘` because the faces map `Cmd` onto the kernel's `ctrl`.

⚠️ **So no menu item may carry a typed-out title or a typed-out chord.** *One
transcription, not two* — a menu that spells `⌘O` next to *Open File…* is a
second place for that fact to go stale, and rebinding the key would leave the
menu lying. The ruled table names **ids only**; everything shown is read.

---

## The open decisions, for the next seat

Not ruled yet — this tick was ground, and the file is written because a
compaction was announced.

- **M-1 — which menus, and which ids in each.** A menu bar cannot be
  registry-driven the way the palette is: fifty-odd commands in a flat list is
  not a way in. It has to be a **ruled table** — the same shape as
  `context_menu::verbs`, which the handoff already notes is a ruled set rather
  than a registry sweep. File / Edit / View / Go is the obvious skeleton.
- **M-2 — greying out.** *Undo* should be dim when there is nothing to undo.
  AppKit asks the target's `validateMenuItem:`, so this is a second method on
  the same class rather than new machinery — but it needs the app state
  reachable from the target, which is the one genuinely fiddly part.
- **M-3 — reaching `DesktopApp` from an ObjC callback.** The action fires from
  AppKit, not from the winit event loop. The safe shape is almost certainly to
  **enqueue a command id** and let the existing event loop run it on its next
  turn, rather than reaching into app state from the callback — the same
  discipline `dialog.rs` observes about not being called from a paint path.
- **M-4 — the terminal face has no menu bar and this task does not give it one.**
  Its way in is the palette. Naming it so it is not silently assumed.

---

## What the ground tick did not do

No code. No `Cargo.toml` change. The value there was G-1: the task was priced
against a claim that is false, and the real task is smaller and differently
shaped than the note said.

---

# ✅ Rulings and step 1 — 13 Aug 2026

Ruled in this seat rather than sent to Tom: these are shape decisions with a
defensible answer, and the standing instruction is to decide, record the
reasoning, and name the revert cost.

## G-6 — ⚠️ `app/menu.rs` already exists and is *not* this

Found on the way in. `apps/iridium-desktop/src/app/menu.rs` is the **context**
menu's host seam — `secondary_pressed`, `drive_menu`, `menu_click`,
`hover_menu`. The menu bar must not take that name. It is `src/menubar.rs`,
and its host seam (when it gets one) is `app/menubar.rs`.

## D-1 — ⛔ No key equivalents in the first slice

An `NSMenuItem` displays a chord by **claiming** it: `setKeyEquivalent` makes
AppKit intercept that chord before the window sees it, and there is no
supported way to show one without taking it.

Claiming ⌘S, ⌘O and ⌘Z moves this face's most-used keys off the winit path —
the one the ten gates cover, the latency instrument measures, and the modal
panel guards sit on — and onto an AppKit path **no test in this repository can
reach**. Wrong trade for a task whose own title is *"for the hand that does not
know the chord"*: the hand that knows it keeps the path that is proven.

Rows carry titles and are clicked. The chord is resolved anyway
(`ResolvedVerb::hint`), so **slice B** — turning it into a real key equivalent
once the action path has been exercised in a live session — is one call and no
new decision. **Revert cost: one line per row.**

## M-1 — ✅ Ruled: six menus, as a table

File, Edit, Selection, View, Go, Help — beside the application menu winit
already installs. Landed as `menubar::ruled()`. Nothing names *Quit*, *About*
or *Hide*: those are already on screen and a second copy is two doors to one
room. A test asserts that.

*Selection* is its own menu rather than folded into *Edit* because
`ast.expandSelection` is the verb hardest to discover and the one most worth
discovering.

## M-2 — ✅ Ruled: greying is **pushed**, not pulled

`validateMenuItem:` would need app state reachable from an ObjC callback,
which is the one genuinely fiddly part of this task. Instead:
`setAutoenablesItems(false)`, and the app pushes the enabled set in
`about_to_wait` — the single moment per turn when the loop is about to become
reachable by a mouse and unable to change on its own.

`verbs::Availability` is the shape of what gets pushed, and it is a **struct,
not a bool**: `read_only` greys only writers, `inert` (a modal panel owns the
session) greys everything. One flag would make one of the two cases wrong.

## M-3 — ✅ Ruled: `EventLoopProxy`, not a shared queue

The action fires from AppKit while `run_app(&mut app)` holds the app, so the
callback cannot touch `DesktopApp` at all. `EventLoop::with_user_event()` gives
an `EventLoopProxy` that both **enqueues and wakes** — `ControlFlow::Wait`
means the loop is asleep — and delivers on `ApplicationHandler::user_event`,
which then calls the existing `run_chosen_command`. Same kernel-first,
undoable path a chord, a palette entry and a context-menu row take.

This costs `impl ApplicationHandler for DesktopApp` becoming
`ApplicationHandler<MenuCommand>` and one change in `run.rs`.

## M-4 — ✅ Ruled and recorded in the module doc

The terminal face gets no menu bar; its way in is the palette.

## ✅ Step 1 — landed `307b9bc0`

`src/menubar.rs` (the ruled table, 9 tests) and `src/verbs.rs` (the resolution,
shared with the context menu, 6 tests). Sabotaged two ways before landing — a
bogus id and a verb repeated across two menus — each caught by its own test.

⭐ **The resolution is now shared.** Both menus answer the same four questions
about a row and an answer given twice can differ; `verbs::resolve` answers them
once and each menu keeps only its own ruled set. `context_menu` lost its
private copy.

## ⚠️ What step 1 turned up: a shipped defect — fixed, `23aecd08`

The desktop face has been showing **every chord with ⌘ and ⌃ swapped**, in the
context menu and the command palette. `KeyLabelStyle::MacGlyphs` mapped the
kernel's `ctrl` to ⌘ and `meta` to ⌃, which is right for the web face and
backwards for this one. The style's own doc predicted the failure — *"a future
face that maps Command to `meta` would need a third style"* — and the face
that arrived took the style whose **name did not warn it**. `keys.rs` asserted
the opposite in prose, untested.

`MacGlyphs` is gone as a name. Two variants now state their assumption:
`MacGlyphsCommandAsCtrl` (web) and `MacGlyphsCommandAsMeta` (desktop).

📌 **The law:** *a name that does not state its assumption is a trap for the
next caller, and a doc comment predicting the trap is not a guard.*

---

## ✅ Steps 2 and 3 — landed `08e981c4`

The `AppKit` half. `apps/iridium-desktop/src/app/menubar.rs` holds it, with a
`#[cfg(target_os = "macos")] mod platform` in the shape `dialog.rs` set.

The path a click takes, which is M-3 as ruled:

```
NSMenuItem → IridiumMenuTarget → EventLoopProxy → user_event → run_chosen_command
```

⭐ **The proxy is the load-bearing choice.** It both enqueues *and* wakes, and
with `ControlFlow::Wait` the loop is asleep precisely when somebody reaches for
a menu. `run.rs` builds it with `with_user_event()` and hands it over, because
only an `EventLoop` can make a proxy — an `ActiveEventLoop`, which is all
`resumed` sees, cannot.

Three things worth carrying:

- ⭐ **The target is leaked, deliberately.** `NSMenuItem`'s target is a *weak*
  reference. A target dropped at the end of `install` leaves every item
  pointing at freed memory and the first click is a crash. It lives as long as
  the menu bar does, which is the process.
- **A tag indexes the command table, not a title.** Titles are shown to a
  person and may be reworded; a tag is an index this code set itself.
- ⛔ **No proxy, no menu bar** — not a menu bar whose items do nothing. Every
  session `cargo test` builds has no proxy and so has no menu, rather than
  drawn-and-dead furniture.

`setAutoenablesItems(false)`: `AppKit`'s own validation would ask an object
that always says yes, so the flag would decide nothing while looking like it
decided something.

### ⚠️ What is proven, and what is not

| | |
| --- | --- |
| ✅ proven | compiles; **links**; `strings` finds `IridiumMenuTarget` and `iridiumMenuAction:` in the binary; all 10 gates green |
| ❌ **not proven** | that the menus appear; that a click runs anything; that the leak is enough to keep the target alive at click time |

A bad selector or a dead target crashes at **click** time, not at install, so
even launching the app proves less than it looks like it would. **Only Tom's
hands close this**, and that is the honest statement of the boundary.

---

## ✅ Step 4 — landed `5b20f5c4`

M-2 as ruled. `about_to_wait` is the hook, `menu_availability` is the answer,
and the push is skipped entirely when the answer has not moved — enabledness
is a pure function of `Availability` and whether a command writes, and the
registry does not change mid-session, so the common case is one comparison.

`inert` is read off `press`'s own branch order (prompt, context menu, palette,
history) so the bar and the keyboard cannot disagree about what modal means.
⚠️ **Explorer focus is deliberately not modality** — it is a focus, the active
editor is still well defined, and greying the bar because a side panel holds
the caret would be greying on a technicality.

The installed items live in a **thread-local**, not on `DesktopApp`: there is
one menu bar per process (`NSApp.mainMenu` is global), and
`Retained<NSMenuItem>` is main-thread-only, so a call from elsewhere finds an
empty list and does nothing — the correct answer rather than a forgettable
check.

Six tests, in `app/tests/menubar.rs`. The one that matters walks **all four**
panels, building a real `Prompt` and `ContextMenu` rather than stand-ins, and
clears each afterwards so a panel that greys the bar and never un-greys it
fails there rather than in a session.

📌 One test was **cut before it landed**: it asserted an invariant and then the
branch that only runs when that invariant is false, so it could only pass. *A
test that cannot fail proves nothing about the thing it names.*

---

## ⛔ What is left — and it is not code

**All four steps have landed.** #108 is code-complete and **cannot be closed
from this seat.**

| | |
| --- | --- |
| ✅ proven | 10 gates green; links; `IridiumMenuTarget` and `iridiumMenuAction:` in the binary; the availability decision has 6 tests |
| ❌ **unproven** | the menus appear · a click runs anything · the leaked target is alive at click time · greying reaches the items |

**The next step is Tom building it and clicking one menu item.** He was asked
on 13 Aug (Meridian) and has not answered. A bad selector or dead target
crashes at *click* time, so launching it here would prove almost nothing.

- **Slice B** — key equivalents, and it is gated on exactly that: D-1 said no
  chords until the action path has been exercised in a live session. If Tom
  confirms a click works, turning them on is `setKeyEquivalent` per row plus a
  modifier mask read from the same `KeyHint` already resolved into
  `ResolvedVerb::hint`. **Do not do it before he confirms.**
