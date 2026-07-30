# Session state — 2026-07-30

Live working state for whoever picks this up. Authoritative roadmap is
`PLAN.md`; requirements and decisions are `THE-CORE-LOOP.md`; the three-face
architecture is `TRIPLE-FACE.md`. This file is the *baton*: what is in flight,
what is outstanding, and what must not be lost.

## Branch and commits

Working branch **`feature/terminal-face`**. `main` is at `c048cf0` and has been
pushed. Commits on the branch, newest last:

| Commit | What |
|---|---|
| `32d7ae9` | docs: corrected layout/GPU line counts in TRIPLE-FACE |
| `0bdcf43` | **feat: GPU-free kernel** — wgpu/glyphon/cosmic-text behind default-on `render` feature |
| `1264cec` | docs: core-loop framing correction + 30 Jul decisions |
| `2e72ecd` | docs: chiron syntax/LSP build-vs-adopt verdict |
| `bce6a45` | **fix: `jump_to_node` must record the branch it travelled** |
| `b017a15` | **fix: GPU-free kernel configuration actually testable** |
| `391d15d` | docs: this baton |
| `a782af7` | **feat: undo-tree branch navigation reachable from `Editor`** + grouping-timeout wiring |
| `cddc4ad` | fix: PLAN crate boundary + CI now *tests* both GPU-free configurations |
| `75a0bce` | **fix: repaired the wasm build**; `web` feature now implies `render` |

Also on `main` already: `f0e8a80` (undo-tree redo fix), `c048cf0` (gitignore).

**Test baseline, all green at `75a0bce`:** 492 (`--no-default-features`), 534
(`--no-default-features --features syntax`), 550 (`--all-features`), 92
bindings, 30 syntax, 16 + 1 elsewhere. `cargo fmt --all --check` clean. Zero
clippy warnings in any file touched (99 pre-existing elsewhere in the crate).

**Correction to the earlier baseline in this file:** the 576/618/634 recorded
against `b017a15` were measured in a working tree that already contained the
registry workflow's uncommitted `commands/` module, so ~91 of those tests were
not on the branch at all. The numbers above were measured in a clean worktree
at `391d15d` plus the committed changes, which is the only way to get an
honest count while the workflow is writing to this checkout.

**Verify in an isolated worktree while the workflow runs.** The recipe that
worked: `git worktree add --detach <scratchpad>/verify-wt <commit>`, copy the
files under test across, run the gates there. The main checkout does not
compile at all while the workflow is mid-write.

## In flight right now

- **Workflow `wy2pfpzd2`** (run id `wf_a24837fa-2de`) — command registry +
  keymap layer (decision D1). **STILL RUNNING as of 15:31 on 30 Jul**, and it
  has moved on from `commands/` to splitting the 1,457-line
  `input/keyboard/mod.rs`: `actions.rs`, `dispatch.rs`, `dispatch_tests.rs`,
  `edits.rs`, `multi_cursor_verbs.rs`, `navigation.rs` are all untracked and
  `input/keyboard/mod.rs` is modified. The checkout does **not** compile mid-write
  (`dispatch_tests.rs` had an `E0596`), so do not read a red build as a real
  failure without checking whether the workflow is between writes. Script at
  `~/.claude/projects/-Users-tom-Developer-ablative-libs-iridium/33ce25a4-8b77-4d27-b58b-a132d9a104af/workflows/scripts/iridium-command-registry-wf_a24837fa-2de.js`.
  Resume with `Workflow({scriptPath, resumeFromRunId: "wf_a24837fa-2de"})`.
  It has produced `crates/iridium-editor/src/commands/` (19 files, ~5,065
  lines, 633→634 tests) — **untracked, not yet committed, not yet reviewed.**
  `crates/iridium-editor/src/lib.rs` is modified by it.
  Known issues to check when it lands: `keymap_tests.rs` was 514 lines (over
  the 500 cap), plus unused imports in `keymap_tests.rs` / `layer_tests.rs`
  and a dead `press` fn in `validation_tests.rs`.

- **Norn reviews complete**, outputs in
  `/private/tmp/claude-501/-Users-tom-Developer-ablative-libs-iridium/33ce25a4-8b77-4d27-b58b-a132d9a104af/scratchpad/norn-out/`
  as `{correctness,architecture,mechanical,test-evidence}.json`, with session
  ids alongside for `--resume`. Fan-out script is `../norn-fanout.sh`.
  Verdicts: architecture `ready_with_comments` (0 majors), correctness
  `not_ready` (1 major — **fixed** in `bce6a45`), mechanical `not_ready`
  (4 majors), test-evidence `not_ready` (3 majors).

## Norn findings — ALL CLOSED

Every major and every non-major from the four review lenses is now fixed and
committed. What they were, and what closing them turned up:

1. **`PLAN.md` factually wrong in two places** — fixed in `cddc4ad`. It routed
   pure layout into `iridium-render` and cited a CI gate on a nonexistent
   `iridium-core`. Closing it also fixed the *gate*: CI was running
   `cargo check` on the GPU-free configuration, and checking a configuration
   whose tests never run is precisely how that configuration became
   unbuildable in test mode without anyone noticing. CI now tests it.
2. **Four non-test `#[allow(dead_code)]`** in the undo module — gone in
   `a782af7`, by making the fields live rather than deleting them.
   `UndoTree::node_info` reports id, parent, children, active child, age and
   description: what an undo-tree view needs to draw itself.
3. **`redo_retraces_the_last_travelled_branch` did not discriminate** — fixed
   in `a782af7`. It used `redo_branch(0)` while the pre-fix `redo` was
   hard-coded to index 0, so it passed against the bug it was written for. It
   now forks three ways and travels into the middle branch, and I verified it
   fails against both wrong answers by neutralising the fix twice: `children
   .first()` yields "A", `children.last()` yields "C", both against an expected
   "B".
4. **No `Editor`-level branching test, no cursor-topology assertion** — fixed
   in `a782af7`, seven tests in `editor/history_nav/tests.rs`. Covering it
   properly meant building the API first (see below), because the only
   `Editor`-level way to switch branches went through `state_mut` and desynced
   the document.

The four non-major architecture comments are also closed, in `75a0bce`:
`web` now implies `render` (it was enabling four dependencies and no API);
`render/minimap/` is named in the kernel manifest (it is load-bearing — public
`MouseHandler` methods take `MinimapRenderer`/`MinimapDimensions`); moving GPU
failure authority out of `IridiumError` is now a Phase 2 item; and the
`--no-default-features --all-targets` failure Norn saw was already fixed by
`b017a15`, re-verified clean.

## Found while closing them — three defects nobody had reported

- **`EditorConfig::undo_group_timeout_ms` did nothing.** Plumbed all the way
  from TypeScript through `JsEditorConfig` into `EditorConfig`, then never
  read: every editor got the hard-coded 500 ms. `Editor::new` now builds the
  history from it and `set_undo_group_timeout_ms` keeps the two in step. This
  closes the "dead `undoGroupTimeoutMs` config" open question.
- **`EditorState::set_content` reverted that timeout** to the default on every
  file open, by replacing the history with `UndoTree::new()`. Replacing the
  tree is correct; losing the configured timeout is not.
- **`b017a15` broke the wasm build.** Removing the `_Placeholder` variant
  orphaned its only use, `wasm.rs:358`. `wasm.rs` is `cfg(target_arch =
  "wasm32")` gated so no native check type-checks it, and I had not run the
  wasm target after the change — CI's existing wasm step would have failed.
  Fixed in `75a0bce`. **Lesson: after touching anything the wasm surface
  consumes, run `cargo check -p iridium-bindings --no-default-features
  --features web --target wasm32-unknown-unknown`.**

## Undo-tree branch navigation — now real, and Tom's question answered

Tom asked for "undo a couple of things, make a change, and then go back up and
down the tree". The engine existed; nothing outside the kernel could reach it.
`crates/iridium-editor/src/editor/history_nav.rs` adds `Editor::redo_branch`,
`jump_to_history_node`, `history_branches`, `history_node` and
`current_history_node`, all routed through the same `finish_history_replay` as
`undo`/`redo`, so cursor restoration, multi-cursor invalidation, search refresh
and event emission are identical however the tree was traversed. A jump across
four edges emits **one** content-changed event, because it has one destination.

`undo_tree/mod.rs` hit 563 lines, so the reporting types and queries moved to
`undo_tree/info.rs` (154). Everything is under the cap.

**Still needs Tom, and is not built:** the *UI*. Keybindings, a visual panel,
and the wasm/napi exposure of these methods. The kernel capability is done and
tested; nothing binds it to a keystroke yet. `UndoNodeInfo` is deliberately
string-keyed for ids so a JavaScript host cannot lose precision on a u64.
`iridium_editor::history::{UndoNodeId, UndoNodeInfo}` are reachable
(`pub mod history`); a convenience re-export from `lib.rs` was left out on
purpose because the registry workflow is rewriting that file.

## Decisions taken (do not relitigate)

- **D1: keymap layer**, non-modal default, modal as a keymap. Tom: *"100% why
  would we pick anything else"*.
- **Soft wrap: yes, default on, toggleable.** Structurally expensive; forces a
  visual-line vs document-line split through the viewport layer and must live
  in the kernel so all three faces agree where lines wrap.
- **Terminal stack: `termina` 0.3.3 + `terminput` 0.5.15 +
  `terminput-termina` 0.3.1.** Tom's call on termina and it is the better
  choice — it has no cell/surface model, so the kernel keeps sole ownership of
  layout. Verified API facts are in the scratchpad `TERMINA-FACTS.md`; both
  spikes compile. `terminput` earns its place because it ships a parser *and*
  an encoder, making the input path testable headlessly with no pty.
- **Mouse: supported, toggleable**, released on teardown.
- **chiron: port the syntax query layer, adopt the LSP crate.** See
  `THE-CORE-LOOP.md` §8. Blocking detail: `tree-sitter` declares
  `links = "tree-sitter"` and the repos resolve 0.26.3 vs 0.25.10, so Cargo
  refuses a graph with both.
- **Extension protocol seam (from Waffles, 2026-07-30):** liminal for anything
  crossing a process boundary, the kernel's public API (later a wasm plugin
  ABI) for anything that does not — and the boundary is exactly the FEEL-1
  line. Out-of-process extensions are liminal participants that register
  commands as data and propose **version-stamped** edits, which the editor
  applies through its reversible `Command` or refuses if the document moved;
  the undo-tree invariant then survives because *the protocol cannot express
  bypassing it*. Streaming (LSP diagnostics, the notebook pill) is where
  liminal genuinely earns its place, because a participant can resume
  mid-stream. **One command registry, two attachment mechanisms, exactly one of
  which is allowed to be slow.** Not started; Tom said do not block on it.
  Reading list: `stack/liminal` `VISION.md` + `docs/design/`, then
  `liminal-sdk/src/remote/participant.rs`, then manifold `a439780`
  `crates/manifold-node/src/mailbox/` with `scripts/e2e-mailbox.sh`.

## Working practice Tom has asked for

- **Norn (GPT-5.6-Sol) en masse is explicitly blessed** — his OpenAI
  subscription makes fleets effectively free, and 50 at a time is fine. Claude
  usage is the tighter budget. Use Norn for bulk mechanical/structural work and
  for review; prefer Opus for implementation. It genuinely catches things: it
  found a real reachable defect in my own undo fix that I had missed, and the
  untestable-configuration hole.
- **Norn needs explicit structural guidance.** Told to split an oversized file
  it once cut it into pieces and rejoined them with `include!` macros. Say
  "break it into a module with sensible logical submodules".
- **Draconian quality standards are real and current** — a 500-line file or a
  clippy warning fails immediately. The existing violations in this repo are
  *not* the standard. Biggest offenders: `iridium-bindings/src/wasm.rs` 3,659,
  `editor/core.rs` 2,267, `input/keyboard/mod.rs` 1,457; 27 Rust files over cap
  in total. (The apparent 7,555-line file is a vendored parser example inside a
  gitignored build cache — not ours.) This backlog is good Norn fleet work but
  **must not** be started while the registry workflow is mid-refactor of the
  same crate.
- Tom communicates via **Meridian**, not the terminal.
- **No silent deferrals.** Anything reduced, dropped or reordered must be
  raised with him explicitly.

## Not yet started

Terminal face (`crates/iridium-tui` + `apps/iridium`) — the thing Tom actually
wants to play with. Blocked on nothing now that soft wrap and the terminal
stack are decided; the prior workflow was stopped before writing any TUI code,
deliberately, so the brief could be rewritten for termina and soft wrap. Note
soft wrap changes the renderer brief substantially versus the original draft.

**It should not start until the registry workflow lands**, because that
workflow is mid-refactor of `input/keyboard/` — the exact seam the TUI's kernel
contract sits on.

## Immediate next steps, in order

1. **Wait for workflow `wy2pfpzd2` to finish**, then review and commit its
   output: `crates/iridium-editor/src/commands/` (19 files) plus the
   `input/keyboard/` split. Known issues to check: `keymap_tests.rs` was 514
   lines (over cap), unused imports in `keymap_tests.rs`/`layer_tests.rs`, a
   dead `press` fn in `validation_tests.rs`, an unused `VerticalDirection`
   import and a never-used `IMPLEMENTED_COMMAND_COUNT` in `actions.rs`, and an
   `unused_mut` in `dispatch_tests.rs`. Do not trust its test counts — measure
   in a clean worktree.
2. **Get Tom's approval on the phase reprioritisation** in
   `THE-CORE-LOOP.md` §4 before touching PLAN's phase order. Still not given.
3. **Ask Tom whether he wants undo-tree branch navigation bound to keys and
   drawn**, now that the API underneath it exists and is tested.
4. **Then the terminal face.**
