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

Also on `main` already: `f0e8a80` (undo-tree redo fix), `c048cf0` (gitignore).

**Test baseline, all green:** 576 (`--no-default-features`), 618
(`--no-default-features --features syntax`), 634 (`--all-features`), 92
bindings, 30 syntax. `cargo fmt --all --check` clean.

## In flight right now

- **Workflow `wy2pfpzd2`** (run id `wf_a24837fa-2de`) — command registry +
  keymap layer (decision D1). Script at
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

## Outstanding Norn majors — NOT yet done

Two of four mechanical majors and two of three test-evidence majors remain.

1. **`PLAN.md` is factually wrong in two places** (mechanical #2). §3 lines
   88–91 assign `gutter/minimap primitives` to `iridium-render`, but the
   ratified boundary (`TRIPLE-FACE.md` §3, and `render/mod.rs` as built) puts
   gutter/minimap/cursor/highlight/viewport in the **kernel** — they are pure
   layout. Lines 107–109 claim CI enforces
   `cargo check -p iridium-core --no-default-features`; **`iridium-core` does
   not exist** and `.github/workflows/ci.yml:23-26` actually checks
   `iridium-editor --no-default-features`. Fix the factual errors. Do **not**
   apply the phase reprioritisation in `THE-CORE-LOOP.md` §4 — that needs
   Tom's explicit approval and he has not given it.

2. **Four non-test `#[allow(dead_code)]` remain in the touched undo module**
   (mechanical #4), now at `history/undo_tree/mod.rs` lines ~31, 48, 51, 57 —
   on `UndoNode.id`, `UndoNode.timestamp`, `UndoNode.description` and
   `UndoNodeInfo`. Zero non-test `#[allow]` is a hard gate. These are
   pre-existing but the constitution requires a touched module brought to
   standard. **Preferred fix:** implement `node_info()` returning
   `UndoNodeInfo`, which makes all four live *and* delivers a piece of the
   undo-tree branch-navigation API Tom wants. `description` is never set to
   anything but `None`; `timestamp` is currently write-only; `id` duplicates
   the `HashMap` key. Deleting them is the alternative but loses future
   capability.

3. **`redo_retraces_the_last_travelled_branch` does not discriminate**
   (test-evidence #2). It uses `redo_branch(0)`, and the pre-fix
   implementation was hardcoded to index 0, so **the test passes against the
   unfixed code**. It would also still pass if the `preferred_child` write in
   `redo_into` were deleted. Rewrite it to travel into a branch that is *not*
   index 0, and verify it fails against the old behaviour. A test that passes
   either way proves nothing — the same standard applied to the workflow
   agents applies here.

4. **No `Editor`-level branching test, and no test asserts cursor topology
   across a branch switch** (test-evidence #3). All undo-tree regressions
   drive `UndoTree` directly with a single zero-position cursor and assert only
   `doc.text()`. Production goes through `Editor::undo`/`Editor::redo`
   (`editor/core.rs:678`, `:703`) which also run `finish_history_replay`.
   `redo_branch` and `jump_to_node` **bypass** that entirely — no cursor
   restoration, no `invalidate_cursor_order`, no `revalidate_vertical_columns`.
   Add an `Editor`-level fork test asserting full cursor state.

Architecture lens returned 4 non-major comments not yet read in detail — read
`norn-out/architecture.json`.

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
