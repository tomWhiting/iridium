# The desktop shape question — tabs, sidebars, and what belongs in the kernel

Opened 5 Aug 2026 (~23:26 local) by Tom, mid-way through the
installability work: *"on web it makes perfect sense to just focus on the
editor component… but on desktop it becomes clear you maybe need the tabs
component, and some kind of sidebar."*

This is a **reopening of a recorded decision**, not a drift.
`DESKTOP-SHELL-PLAN.md`, "What v1 deliberately does not contain", parks
*multiple windows/tabs (one file per window, like the TUI)*. Reopening it
is fine; doing so without noticing would not be.

Nothing here is decided. Ground is verified, options are priced, and the
questions carry IDs so they can be ruled on rather than discussed twice.

---

## Verified ground (read from the tree, 5 Aug 2026)

- **Nothing in the estate owns more than one `Editor`.** Not
  `apps/iridium-desktop`, not the web face, not the TUI. Grep for
  `Vec<Editor>`/`struct Workspace`/`tabs` returns only the `Tab` *key* in
  `keys.rs` and `search.rs`. One document per instance, everywhere. Tabs
  and sidebars are **new structure, not wiring.**
- **`crates/iridium-file` already exists and is the shared file layer** —
  `TextFile` compares the *exact bytes* last agreed with disk, not an
  mtime/size stamp, because a formatter or `git checkout` inside one
  filesystem tick produces identical stamps and different contents. Any
  multi-document model inherits this rather than re-deciding it.
- **The desktop shell takes files from `std::env::args_os()` only**
  (`apps/iridium-desktop/src/run.rs:54`). No Apple-Event open-documents
  handling, no `WindowEvent::DroppedFile` arm in `app.rs`.

## The failure mode this design exists to avoid

A feature invented separately in each face. **This is not hypothetical —
there is a live instance in the tree today:** `wasm.rs:2827` hand-rolls
pixel-to-line-index arithmetic because the kernel's bit-exactly tested
`pixel_to_index` is `pub(crate)` and unreachable from `iridium-bindings`.
The two agree **only because the same person wrote both** (task #38).

Build tabs three times and you get three of those. The disagreements
would be things like *which tab becomes active when you close the current
one* — invisible in review, infuriating in daily use, and impossible to
attribute when it differs between the browser and the desktop.

## The proposed split — same rule the palette already follows

`THE-CORE-LOOP` plan §1.4 settled this once for the command palette:
ranking is **behaviour**, so it lives in Rust; only the pixels are
per-face. *"Same query, different top result in terminal vs web"* is a
bug. The identical argument applies here.

- **KERNEL** — a `Workspace` owning N documents: active index, order,
  per-document dirty and staleness (delegated to `iridium-file`), and the
  verbs (`workspace.nextTab`, `closeTab`, `reopenClosed`, …) **including
  the rules** for what happens on close. All behaviour.
- **FACE** — the tab strip's pixels. The sidebar's pixels. Nothing else.

## ★ The constraint that decides the sidebar: the browser has no filesystem

A file-tree sidebar cannot work in the browser the way it works natively.
The File System Access API is Chromium-only and permission-gated. **This
is the same wall as syntax navigation** (`THE-CORE-LOOP` finding 1: the
web face's tree-sitter lives in a JS worker whose protocol carries only
highlight spans).

So "sidebar" is **three different features** wearing one word:

| | what it is | web story | cost |
|---|---|---|---|
| **(a)** | filesystem tree | none — permanently native-only | highest; forks the faces |
| **(b)** | document outline from the syntax tree | works wherever the tree does | rides on already-planned work |
| **(c)** | open-documents list | works everywhere | nearly free once `Workspace` exists — it is tabs rendered vertically |

## Decisions for Tom

- **S-1** — Reopen the no-tabs decision? If yes, `Workspace` in the
  kernel is the first step and everything else follows from it.
- **S-2** — Which sidebar: (a), (b), (c), or more than one? (c) is
  nearly free, (b) is cheap and shared, (a) is the only one that
  permanently forks web from desktop.
- **S-3** — If tabs land, what does "Open With → iridium" do: add a tab
  to the existing window, or spawn a new one? **This blocks the Apple
  Event work and nothing else.**
- **S-4** — Does the web face get tabs too, or is it deliberately the
  single-editor-component embed it is today? The `Workspace` split above
  costs nothing extra if the answer is "later"; it costs a rewrite if the
  answer is "never, so we hard-coded one document".

## What is NOT blocked by any of this

`install.sh` and drag-and-drop file opening. Packaging and file-opening
do not care how many documents a window holds. Proceeding with both.
