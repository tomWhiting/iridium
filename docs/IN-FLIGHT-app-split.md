# IN FLIGHT — the `app.rs` split (task #49)

**Status at the moment of writing: HALF DONE. The tree does not compile.**
`apps/iridium-desktop/src/app.rs` still exists *and* `app/mod.rs` exists, which
is error E0761 (module found at both paths). That is expected and is fixed by
step 1 below.

Tom's authorization, 6 Aug via Meridian: *"Yeah, happy for you to go ahead."*

---

## What is already written

`apps/iridium-desktop/src/app/` — **fourteen source files, all written**:

| file | holds |
|---|---|
| `mod.rs` | module doc + declarations + `pub use` only |
| `state.rs` | `Flow`, `UNTITLED`, `DesktopDocument`, `DesktopApp`, `Debug`, `failure`, `report_latency`, `is_dirty`, `document_is_dirty`, `request_redraw`, the `#[cfg(test)]` accessors |
| `startup.rs` | `BASE_FONT_SIZE`, `FONT`, `StartupError`, `Options`, `Shell` + `Shell::open`, `DesktopApp::new` |
| `title.rs` | `TITLE`, `refresh_title`, `title_for` |
| `keyboard.rs` | `press` + the modal ladder, `run_chosen_command`, `consume`, `search_action`, `is_paste_chord` |
| `host_commands.rs` | `run_host_command`, `dispatch_host_command`, `run_workspace_command` |
| `tabs.rs` | `close_active_tab`, `ensure_a_tab_is_open`, `after_tab_change`, `tab_strip_content`, `tab_strip_press`, `tab_at` |
| `files.rs` | `save`, `save_as`, `dropped`, `open_file`, `request_quit`, `language_of` |
| `clipboard.rs` | `clipboard`, `clipboard_handle`, `clipboard_store`, `clipboard_text` |
| `pointer.rs` | `pointer_moved/pressed/released`, `dismiss_modal_panel`, `pointer_is_on_*`, `place_caret_under_pointer`, `blurred`, `hit_test`, `apply_mouse`, `wheel` |
| `menu.rs` | `secondary_pressed`, `drive_menu`, `menu_click`, `hover_menu`, `painted_menu` |
| `viewport.rs` | `SCROLL_PADDING`, `scroll_by`, `ensure_caret_visible`, `clamp_scroll`, `viewport_window`, `sync_kernel_viewport`, `sync_top_inset`, `resized`, `rescaled` |
| `paint.rs` | `redraw`, `strip_content`, `panel_contents` |
| `handler.rs` | `impl ApplicationHandler` — the winit seam |

`DesktopApp`, `Shell` and `DesktopDocument` fields are all `pub(super)`; every
method crossing a file boundary is `pub(super)`, the rest stayed private.

---

## STEP 1 — the one thing left to write: `app/tests/`

`app/mod.rs` already declares `#[cfg(test)] mod tests;`, so **`app/tests/mod.rs`
must exist or nothing compiles.**

The source is the old `mod tests { … }` block at **`app.rs` lines 2176–3270**
(1,094 lines), which is over the 500-line bar and splits into four by theme.
Extract the line ranges **verbatim** — do not retype them — then let
`cargo fmt --all` fix the now-wrong indentation (rustfmt reindents code and
leaves string literals byte-identical; `wrap_comments`/`normalize_comments` are
both off in `rustfmt.toml`, so comment *text* is safe too).

| new file | old `app.rs` lines | contents |
|---|---|---|
| `tests/support.rs` | 2189–2223, 2380–2401, 2851–2882 | `chord`, `press`, `ctrl_s`, `open`, `type_into`, `ctrl_alt`, `meta`, `fixture`, `ctrl_shift`, `ctrl_w`, `front` — all `pub(super)` |
| `tests/saving.rs` | 2225–2378 | the 7 save/close tests |
| `tests/panels.rs` | 2402–2849 | palette, context menu, modal dismissal, history, search, language, title tests |
| `tests/tabs.rs` | 2883–3270 | the Tabs and The-tab-strip sections |

`tests/mod.rs` carries declarations only (`mod panels; mod saving; mod support;
mod tabs;`).

**Import fixes each test file needs** (the old block used `use super::{…}`,
which now means the wrong module):

- `use crate::app::state::{DesktopApp, Flow};`
- `use crate::app::title::title_for;` (only where `title_for` is used)
- `use crate::app::startup::Options;`
- plus the originals: `std::path::PathBuf`, `iridium_editor::{KeyCode, KeyEvent,
  Modifiers}`, `iridium_file::test_support::TempDir`, `crate::prompt::Prompt`,
  `crate::tab_strip::TabHit`, `iridium_editor::workspace::Node`
- helpers come from `use super::support::*;`

`DesktopDocument` is `pub(super)` in `app::state` — visible to `app::tests::*`
because they are descendants of `app`. Same for every `pub(super)` method.

## STEP 2 — delete the old file

`rm apps/iridium-desktop/src/app.rs` (committed at `b1ab351`, so recoverable).

## STEP 3 — compile and fix

`cargo check -p iridium-desktop --all-features`, then with `--tests`. Expect
misses in the `pub(super)` marking and in per-file imports; the compiler names
each one. Then `cargo fmt --all`.

## STEP 4 — the six gates

Listed in `SESSION-STATE.md`. Each unpiped, exit status checked on its own.

## STEP 5 — prove it is a pure refactor

**This is not optional, and the suite alone does not do it.** Both diffs, exactly
as for the compositor split (`cd033b6`):

```bash
norm() { grep -vE '^\s*(//|$|use |mod |pub use )' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | sort; }
git show HEAD:apps/iridium-desktop/src/app.rs | norm > /tmp/…/old.txt
cat apps/iridium-desktop/src/app/*.rs apps/iridium-desktop/src/app/tests/*.rs | norm > /tmp/…/new.txt
diff /tmp/…/old.txt /tmp/…/new.txt
```

Only acceptable differences: `pub(super)`/`pub` prefixes, the added `impl
DesktopApp {` headers and their braces, `impl ApplicationHandler for DesktopApp {`,
rustfmt reflows of signatures that changed width, and the `#[cfg(test)] use`
line in `state.rs`. **No statement may be added, dropped or altered.**

Then the public-surface diff (this is the one that catches a `pub` quietly going
private — compiles fine inside the crate, breaks `run.rs`):

```bash
git show HEAD:apps/iridium-desktop/src/app.rs | grep -oE 'pub (const )?fn [a-z_]+|pub (struct|enum) [A-Za-z]+' | sort > old_api.txt
cat apps/iridium-desktop/src/app/*.rs | grep -oE 'pub (const )?fn [a-z_]+|pub (struct|enum) [A-Za-z]+' | sort > new_api.txt
diff old_api.txt new_api.txt
```

Consumers to keep working: `run.rs` uses `crate::app::{DesktopApp, Options}`;
`lib.rs` has `pub mod app;`.

## STEP 6 — commit, push, tell Tom

Commit message in the shape of `cd033b6`. Then `docs/SESSION-STATE.md` gets the
same treatment the compositor split got, this file is **deleted**, and task #49
is closed.

---

## Do NOT forget

- Report to Tom through the Meridian `send` tool (`dm:c9255b2a-5731-4d17-8124-e3bfa2224186`,
  this seat is "Doug") or it did not happen.
- Never `git stash` / `git checkout --` / `git restore` / `git reset` /
  `git clean` / `git worktree` in this checkout.
- `git diff --no-ext-diff` (difftastic is wired in).
- Already told Tom: nothing is visibly different in the installed app; the last
  thing that changed pixels was the tab strip. Do not reinstall until the tree
  popover shows something.
