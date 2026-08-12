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

## What this tick did not do

No code. No `Cargo.toml` change. The value here is G-1: the task was priced
against a claim that is false, and the real task is smaller and differently
shaped than the note said.
