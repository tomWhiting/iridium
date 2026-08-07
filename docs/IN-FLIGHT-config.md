# IN FLIGHT — settings and configurable keymaps (task #59)

**Status: ground verified, nothing written yet.** Everything through
`641fa97` is landed and pushed. **The binary is NOT installed** — Tom had the
app running (pid 74537) when `641fa97` landed, and the no-swap-under-a-live-
session rule holds. `pgrep -x iridium-desktop` first; install only via
`apps/iridium-desktop/bundle/install.sh`.

## Tom's ruling, 7 Aug 03:06

> "I'd probably go for the file first and then the UI over it, but both
> would be good."

So: **a text config file now**, a settings UI over it later. Do not start the
UI.

## Ground — all verified by reading the source, not assumed

| fact | where |
|---|---|
| `EditorConfig` **already derives serde**, with `#[serde(default = "…")]` per field and doc comments explaining each | `crates/iridium-editor/src/editor/config.rs:25` |
| `KeyBinding::parse(sequence, command) -> Result<Self, KeymapError>` — the text form "a configuration file or a rebinding UI works in", its own words | `crates/iridium-editor/src/commands/binding.rs:291` |
| `KeyBinding::parse_sequence(text)` handles space-separated chords, e.g. `"ctrl-k ctrl-c"` | `binding.rs:274` |
| `Keymap::new(name)` · `push(binding)` · `extend(bindings)` | `crates/iridium-editor/src/commands/keymap.rs:71,108,122` |
| `KeyboardHandler::push_validated_keymap(keymap, registry)` — **documented as the path a host loading a keymap from configuration should take** | `crates/iridium-editor/src/input/keyboard/keymap_api.rs:97` |
| `Workspace::push_keymap` / `register_command` exist and apply to **every open editor and every future one** | `crates/iridium-editor/src/workspace/settings.rs` |
| `FaceSetupError { Registry, Keymap }` is the existing union for "a face's keymap was refused" | `workspace/settings.rs:33` |
| `KeymapError` variants: `EmptySequence`, `UnknownCommand`, `UnparsableStroke`, plus a prefix-shadowing one | `crates/iridium-editor/src/commands/error.rs:26` |
| `serde` **is** a workspace dependency; **`toml` is NOT** | root `Cargo.toml` |

**The kernel half is already built.** This work is location, reading,
parsing, and reporting — not new keymap machinery.

## The decision taken: a new crate, `crates/iridium-config`

Native-only, exactly as `iridium-explorer` and `iridium-file` are, and for
the same stated reason those give: **a browser has no config file**, so the
wasm bundle must never carry a TOML parser. Both native faces (desktop and
TUI) depend on it; `iridium-editor` does not.

What can actually drift between faces is already in the kernel — which chords
mean what (`KeyBinding::parse`), and what settings exist (`EditorConfig`).
What is left is genuinely native-face work with two consumers, which is what
a shared native crate is for.

## Shape

```text
crates/iridium-config/
  lib.rs        # declarations + the doc below
  location.rs   # $XDG_CONFIG_HOME else ~/.config/iridium/config.toml,
                # env injected as arguments so it is testable
  schema.rs     # UserConfig { editor: EditorConfig, keys: BTreeMap<String,String> }
  load.rs       # text -> (Settings, Vec<Problem>)
  keymap.rs     # the keys map -> Keymap, collecting problems
  problem.rs    # Problem { section, detail } + Display
```

## The rules that must hold, in priority order

1. **A bad config never stops the editor.** Every problem is *collected*, not
   returned as a fatal error. Anything unparseable falls back to its default
   and the editor starts.
2. **Per-section isolation.** `toml::from_str` into a `toml::Table` first,
   then convert each section separately — so a typo'd boolean in `[editor]`
   does not silently take every keybinding with it. "I changed one setting
   and all my keys stopped working" is exactly the invisible-until-it-bites
   failure this codebase exists to avoid. A whole-file *syntax* error is
   unavoidably fatal to the file; that one is reported and everything
   defaults.
3. **A refused binding is reported, never swallowed.**
   `KeymapStack::check_cross_layer_shadowing` rejects a longer sequence
   sharing a live prefix — so a user binding `ctrl+k x` **will** be refused
   while bare `Ctrl+K` is bound to the palette. That refusal has to reach the
   prompt strip with the reason, or the key is simply dead and nobody knows
   why.
4. **No file is not a problem.** Absent config = defaults, silently. Only a
   file that exists and is wrong says anything.

## The trap that will bite if it is not handled

**The desktop face maps Cmd to `meta`; the web face forwards Cmd as `ctrl`**
(`packages/@iridium/core/src/controller/index.ts:837`, and desktop
`keys.rs:44`). A config file naming chords therefore means *different things*
in two faces unless one spelling is picked and translated at the edge. Decide
this explicitly and write it down in the crate's module doc; do not let it be
discovered.

## Still open with Tom

- Enter on a folder in the **unfiltered** explorer tree: descend and re-root,
  or toggle? (Asked three times. `⌘↓`/`⌘↑` now do the re-rooting, so this
  only ever *adds* a way in.)
- Does the explorer replace `⌘O` or sit beside it?
- Task #58, the editable oil buffer, is blocked on the first of those.
