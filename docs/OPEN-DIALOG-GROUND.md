# #107 — Open File / Open Folder / Set Project: verified ground

Written 9 Aug 2026 mid-investigation. Everything here was read out of the
crates on this machine, not recalled. Nothing has been built yet.

## Tom's ask, verbatim (Meridian, 9 Aug 01:32Z)

> "There's no open button, there's no open directory, there's no set project or
> anything like that."

## THE DEPENDENCY QUESTION IS SETTLED: use `NSOpenPanel`, not `rfd`

`cargo tree -p iridium-desktop -i` proves **both** objc2-app-kit versions are
already compiled into this crate:

```
objc2-app-kit v0.2.2 └── winit v0.30.13 └── iridium-desktop
objc2-app-kit v0.3.2 └── arboard v3.6.1 └── iridium-desktop
```

So adding `objc2-app-kit = "0.3"` + `objc2-foundation = "0.3"` as **direct**
dependencies adds **zero new crates to the build** — only new edges to crates
already there. `rfd` would add its own stack for the same result. Use the 0.3
line, matching arboard, not winit's 0.2.

## The exact API, read from `~/.cargo/registry/src/*/objc2-app-kit-0.3.2/`

`src/generated/NSOpenPanel.rs`:
- `NSOpenPanel::openPanel(mtm: MainThreadMarker) -> Retained<NSOpenPanel>` (:128)
- `.setCanChooseFiles(bool)` (:170), `.setCanChooseDirectories(bool)` (:152)
- `.setAllowsMultipleSelection(bool)` (:~158)
- `.URLs(&self) -> Retained<NSArray<NSURL>>` (:134)

`src/generated/NSSavePanel.rs` — **`runModal` lives on the superclass**:
- `pub fn runModal(&self) -> NSModalResponse` (:433). Doc: "Presents the panel
  as an application modal window. Returns after the user has closed the panel.
  Returns `NSModalResponseOK`, `NSModalResponseCancel`, or if the panel fails
  to display, `NSModalResponseAbort`."

`NSModalResponseOK` is re-exported from `objc2_app_kit` (generated/mod.rs:7299,
originally `__NSWindow`).

`objc2-foundation-0.3.2/src/generated/NSURL.rs`: `pub fn path(&self) ->
Option<Retained<NSString>>` (:1314).

### Cargo features needed

`NSOpenPanel` is gated on `all(feature = "NSPanel", "NSResponder",
"NSSavePanel", "NSWindow")`. So the dependency must enable at least those four
features plus whatever `NSOpenPanel` itself needs.

### The three traps

1. **`MainThreadMarker` is required and must be honest.** `openPanel` takes one.
   Get it with `MainThreadMarker::new()` → `Option`, which checks at runtime.
   That `None` is the seam: a call from the wrong thread must return
   `Unsupported`, never panic.
2. **`runModal` blocks the winit event loop** until the panel closes. That is
   correct and expected for a modal picker — the OS spins its own run loop — but
   it means the frame thread stops, so nothing that must keep ticking (the
   explorer's `poll`) can be relied on across the call.
3. **Three outcomes, not two.** The return type must distinguish
   `Chosen(PathBuf)` / `Cancelled` / `Unsupported`. Returning `Option<PathBuf>`
   makes "this platform has no dialog" indistinguishable from "the user pressed
   Cancel", which is the same success-only-channel defect as everywhere else.

## The design decision NOT yet taken, and the reasoning so far

⚠️ **`⌘↓` in the explorer re-roots the panel but does NOT change the session's
project.** `DesktopApp::project` (from #101) is what `explorer_root()` returns
first, so after walking into a folder with `⌘↓`, closing and reopening the panel
snaps back to the *original* project root. `the_project_root_survives_closing_and_reopening_the_panel`
asserts exactly that, deliberately.

That is arguably right and arguably "there's no set project". The resolution I
was heading for, which keeps both properties:

- `⌘↓` / `⌘↑` stay **transient** walking. Nothing changes.
- A separate explicit **Set Project** command pins the session to the selected
  folder — i.e. writes `DesktopApp::project`.

So three commands, all new, in `apps/iridium-desktop/src/commands.rs` beside
`CONFIG_EDIT`:

| id | title | key |
|---|---|---|
| `file.open` | Open File… | `⌘O` |
| `project.open` | Open Folder… | `⌘⇧O` |
| `project.set` | Set Project to Selected Folder | palette-only |

`⌘O` and `⌘⇧O` are free — checked against `BINDINGS`/`MAC_CHORDS` on 9 Aug; the
kernel binds neither `Ctrl+O` nor `Ctrl+Shift+O`. **Re-verify before binding:**
`every_ctrl_chord_a_mac_hand_reaches_for_has_a_meta_spelling` and
`no_binding_collides_with_the_default_keymap_except_the_mac_chords` will both
have an opinion, and these are new ids rather than mac spellings of kernel
verbs, so they belong in `BINDINGS` (which must override nothing).

## Wiring already in place — reuse, do not rebuild

- `DesktopApp::open_file(&Path)` — `app/files.rs:118`. Opens a real `TextFile`
  into a tab, sets the language, activates it. This is what `file.open` calls.
- `DesktopApp::project: Option<PathBuf>` — `app/state.rs`. What `project.open`
  and `project.set` write.
- `DesktopApp::explorer_root()` — `app/host_commands.rs`, returns
  `project::chosen_root(project)` when a project is set.
- `FileExplorer::open(root, crawl)` / `crate::project::chosen_root(PathBuf)` —
  how the panel is built from a root.
- `FileExplorer::root_path()` and `selected_path()` — `file_tree/panel.rs`, what
  `project.set` reads to find the folder to pin.
- `EXPLORER_TOGGLE_PANEL` handling — `app/host_commands.rs:64`.

## Menu bar — NOT started, and it is the other half of the ask

winit does **not** do menus. Options are `muda` (new dependency, not in the
tree) or `NSMenu` directly via the objc2-app-kit already present. The same
zero-new-crates argument applies, so `NSMenu` is likely right for the same
reason. Nothing verified about it yet — treat this paragraph as unchecked.

## State at the time of writing

- `a49ca46f` is HEAD, pushed. Working tree clean but for untracked
  `.claude/skills/`.
- #103, #104, #105, #106 landed and are installed; ten gates green.
- The app is installed at `/Applications/iridium.app`, `iridium` is on the PATH
  at `~/.local/bin/iridium`, and the terminal face is preserved as
  `~/.local/bin/iridium-tui`.
- A `Monitor` on `pgrep -x iridium-desktop` fired and ended; none is armed.
