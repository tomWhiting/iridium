# Iridium — Session Handoff

**Written 2026-07-16 for a fresh model picking up the work.** This is a
point-in-time snapshot of *where things stand and how to continue*, not a
spec. The authoritative roadmap is `docs/PLAN.md`; the hard rules are
`CLAUDE.md`. Read both. This document tells you what those two don't: the
working process, the current frontier, and the traps.

---

## 1. What Iridium is

A GPU-accelerated text editor in Rust (wgpu 28 / glyphon 0.10 / cosmic-text
0.16 / ropey 2.0 / tree-sitter 0.26), part of Tom's "ablative stack"
ecosystem (Meridian messaging, Norn agent harness, Frame, etc.).

The product identity is **Dieter Rams**: "less, but better." One editor with
the *feel* of Zed, spanning **terminal → desktop → web from a single
codebase**. Tom lives in the terminal ~90% of the time; the terminal face is
the point, and its target tone is nvim. The design rule is a toaster that
never dies — an editing core so good it needs nothing bolted on.

**AI-separable by construction.** Tom loves AI but insists the editor and the
AI be *separable*: the editor must be excellent standalone. AI hosts (Norn,
Meridian) attach through the public API only — never woven into the core. Do
not add AI features to the kernel. Ever.

Target crate architecture (Phase 2+, not yet built): `iridium-core`
(GPU-free editing kernel) + `iridium-render` + `iridium-tui` + `iridium-desktop`
+ `iridium-web` + `iridium-lsp`. Today it's still `iridium-editor` (core +
render together), `iridium-syntax`, `iridium-bindings`.

---

## 2. Who Tom is, and how he works

- **Informal, voice-to-text, expletive-friendly.** Don't be stiff back, but
  don't perform it either.
- **Comms are via Meridian**, not the terminal, primarily. Use the
  `mcp__meridian-remote__send` tool (DM `to: "Tom"`). He reads Meridian, not
  the session transcript — anything you want him to see goes through send.
- **No silent deferrals. This is the single most important rule.** If you
  want to defer, drop, or reduce *anything*, you must **stop and ask**. His
  words: "it's not defer, it's MOVE where it is in the plan." There is no
  "minor issue" category. A thing is done properly or it's an explicit,
  visible decision to move it. Violating this is the fastest way to lose his
  trust.
- **Hard hygiene bar** (see §5). ~500-line module cap, zero unwrap/expect/
  panic in non-test code, zero clippy bypasses in non-test code.
- **Never economize on review or reasoning.** His explicit directive: Norn on
  xhigh or max is "totally fine, definitely not overkill." His worry is the
  *opposite* of spending too much — he wants maximum thoroughness. Don't
  downgrade effort to save tokens unless he says so.
- **Autonomous execution is expected.** He's often not watching. For
  reversible work that follows from the plan, proceed. Stop only for
  destructive actions or genuine scope decisions.

---

## 3. Current state (as of commit `8a2ecb9`)

- **Branch:** `feature/lsp-hover-completion` (yes, the branch name predates
  this work; don't be confused by it). 18 commits ahead of the prior
  baseline. Working tree clean except untracked `.claude/skills/` (the Norn
  skill — intentionally untracked, leave it).
- **Tests:** 538 in `iridium-editor` (baseline was 253), 50 in
  `iridium-bindings`, 17 in the syntax-worker (bun). All green.
- **Phase 0 and all of Phase 1 are landed and committed.** Nothing is
  in-flight. The last stint ended cleanly at Tom's instruction ("finish and
  stop").

### What Phase 0 + 1 delivered (commit-by-commit context)

- Untangled a large WIP into thread commits; CI skeleton; gitignore for
  session state.
- **Wave 1 — isolated correctness fixes:** Unicode offset desync in
  case-insensitive search; `SpanIndex::len()` overreporting; undo-tree
  quadratic clone-and-nest flattened; regex capture-group expansion with
  **verify-or-skip** semantics (stale matches are skipped, never guessed —
  this was a real corruption bug Norn caught).
- **Wave 2 — multi-cursor made real** end-to-end: edit ops, navigation,
  paste, and undo that restores cursor topology. Lone-CR caret-advance
  unified into `Position::advanced_through` (was 4 duplicated buggy copies).
- **Wave 3a:** `EditorConfig` wired into the keyboard handler; auto-pairs,
  indent/outdent, auto-indent, bracket-block/code-fence Enter repatriated
  from TypeScript to Rust.
- **Wave 3b:** the web controller rerouted through Rust key handling; ~350
  lines of JS "editing brain" deleted; rope-byte edit tracking added (which
  killed most of the UTF-16 bug class early).
- **Line ops + comment toggling:** move/duplicate/delete/join lines; Ctrl+/
  and Shift+Alt+A comment toggle with a per-language table.
- **Wave 4:** syntax-worker UTF-16 ↔ rope-byte coordinate conversion, proven
  against web-tree-sitter with runtime probes (web-tree-sitter indexes in
  UTF-16 code units; the rope is UTF-8 bytes; conversion happens at the
  worker boundary in `encoding.ts`).
- **Multi-cursor verbs (final commit `8a2ecb9`):** add cursor above/below
  (Ctrl+Alt+Up/Down, per-cursor sticky columns), select all occurrences
  (Ctrl+Shift+L), skip occurrence (public method, no binding — see §7 D1),
  undo last cursor (Ctrl+U). Plus an AltGr-safety fix (see §6), a
  `Document::revision` counter guarding the cursor-addition stack, and a
  cursor-merge fix keeping adjacent occurrence selections distinct.

---

## 4. The working loop (this is the process — follow it)

Every wave this stint used the same loop, and **Norn caught a genuine defect
in every single wave.** It is not optional ceremony; it works.

1. **Implement** — either directly, or via a background agent / dynamic
   Workflow. Tom's directive was opus executors for workflow implementation
   agents.
2. **Verify the claims yourself** — do not trust an agent's summary. Read the
   diff, run `cargo test -p iridium-editor --all-features`, run clippy on the
   touched files. Ground-truth everything.
3. **Norn adversarial review** — via the `norn` skill (GPT-5.6-Sol, xhigh).
   Norn reviews the uncommitted diff and returns structured findings. It is
   adversarial and good; take its majors seriously.
4. **Fix findings** — at the root, each with a regression test that
   reproduces the finding's exact scenario. Re-run Norn (resume the session)
   to confirm closure.
5. **Commit** — only when tests are green, fmt clean, and no new clippy
   warnings in touched files.

### The Norn recipe

The skill lives at `.claude/skills/norn/`. Invoke via the `Skill` tool
(`norn`) or drive the CLI directly. The review shape:

```
norn --print --model gpt-5.6-sol --reasoning-effort xhigh --fast \
  --working-dir <repo> --workspace-root <repo> \
  --allowed-tools "read,search,lsp,bash" \
  --append-system-prompt "$(cat .claude/skills/norn/instructions/review/base.md \
                               .claude/skills/norn/instructions/review/correctness.md)" \
  --output-schema .claude/skills/norn/schemas/review.schema.json \
  --output-format json
```

Envelopes are saved to `~/.norn/delegations/claude-review-*.XXXXXX`. Sessions
are **resumable** via `--resume $SESSION_ID` — use this to verify fixes
against the same reviewer that found them. Use *distinct* envelope names per
review to avoid stale-envelope confusion.

### Workflows

The `Workflow` tool runs deterministic multi-agent orchestration in the
background. Tom has explicitly opted into these ("use dynamic workflows using
opus to execute stuff"). The pattern that worked: an implement stage → a Norn
review stage → a fix loop (bounded rounds) → a verify stage, with opus
executors. Scripts persist under the session dir and are resumable via
`{scriptPath, resumeFromRunId}` if a run dies (usage limits killed a couple
of runs this stint; resume replayed cached agents instantly). **Note:** the
last workflow's fix loop exhausted its 3 rounds and still left 2 majors — I
fixed those by hand. Don't assume a workflow's internal loop caught
everything; always run a final independent Norn pass.

---

## 5. The quality constitution (hard gates — `CLAUDE.md` + `PLAN.md` §2)

- **Mission-critical standard.** The stated bar: would you trust this with
  patient records, financial transactions, legal documents? Financial/legal/
  healthcare infra is the framing. No lazy code, no shortcuts, no partial
  implementations, no deferred work.
- **Zero `unwrap()`/`expect()`/`panic!()` in non-test code.** Clippy warns on
  these. Test modules MAY carry one scoped
  `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` — this
  was an amendment (commit `267ac48`) flagged to Tom for veto; he hasn't
  vetoed but it's still technically open (§7).
- **Zero `#[allow]` in non-test code.**
- **~500-line module cap.** This is why `keyboard/` and others are directory
  modules with many small files.
- **All public items documented** (`missing_docs = "warn"`).
- **Aggressive clippy** (pedantic + nursery). CI currently runs clippy
  report-only (`continue-on-error`) with a per-phase `-D warnings` ratchet
  planned — but the *standing rule* is zero new warnings in any file you
  touch. There is a pre-existing warning backlog in untouched render/view
  files; don't add to it.
- **Tab width 4, max line width 100, edition 2024, Rust 1.85+.**
- **Command-sourced:** every mutation goes through a reversible `Command`
  (Insert/Delete/Replace/SetSelection/Compound). The undo tree preserves
  branches — never lose work. If you're mutating state outside a Command,
  you're probably doing it wrong.
- **Tests are document-state-level** — they drive the real handler, apply the
  produced command to a real document, and assert the full resulting cursor/
  document state. Not unit-mock theatre.

### BANNED PORTS — never run a server on 3000, 3030, 8000, 8080.
Use obscure ports (12223, 14567, etc.). This bites people constantly.

---

## 6. Gotchas that will bite you

- **The wasm module is target-gated** (`cfg(target_arch = "wasm32")`), so
  native `cargo check` never type-checks it. A change to the `WebEditor`
  surface in `crates/iridium-bindings/src/wasm.rs` can compile natively and
  be broken on wasm. CI now has a wasm32 check step (added `8a2ecb9`) with the
  exact wasm-pack feature set (`--no-default-features --features web`). Run it
  locally after touching bindings:
  `cargo check -p iridium-bindings --no-default-features --features web --target wasm32-unknown-unknown`.
  A full `wasm-pack build crates/iridium-bindings --target web --features web
  --no-default-features` regenerates the JS glue + `.d.ts` (output at
  `crates/iridium-bindings/pkg`, which is gitignored).
- **UTF-16 vs UTF-8.** web-tree-sitter is UTF-16 code units; the rope is
  UTF-8 bytes; `Position.column` is **UTF-8 code points** (not bytes, not
  graphemes). Conversion lives in `packages/@iridium/syntax-worker/src/
  encoding.ts`. When writing Unicode tests, columns are code points.
- **AltGr = Ctrl+Alt on many non-US layouts.** This collides with the
  add-cursor chord. There's now an `alt_graph` bit on `Modifiers`, plumbed
  from `getModifierState("AltGraph")` in the controller → the six-argument
  wasm `handleKeyEvent` → the dispatch guard (which requires `!alt_graph`).
  If you add a Ctrl+Alt chord, respect this.
- **macOS key semantics** are translated controller-side in
  `translateKeyEvent` (Option→ctrl word ops, Cmd+arrows→Home/End/doc,
  alt-chorded letters normalized to the physical base letter via `e.code`
  because macOS turns Shift+Option+A into "Å"). The Rust core's contract is
  base logical letters + modifiers.
- **Sticky columns and the cursor-addition stack** are subtle. Sticky columns
  (preferred column for vertical movement) are validated by *exact cursor-
  state identity* plus a document revision, and self-invalidate on any
  non-vertical change. The addition-order stack (for Ctrl+U / skip) is
  guarded the same way. If you touch multi-cursor code, read the long doc
  comments on `KeyboardHandler`'s fields and `note_operation` in
  `crates/iridium-editor/src/input/keyboard/mod.rs` — they explain the
  invalidation model. The trap Norn found twice: a selection that round-trips
  back to a byte-identical state can revive a stale snapshot. Every actual
  selection change must invalidate.
- **No DOM test infrastructure** exists (no jsdom/happy-dom). The only TS
  tests are pure-function bun tests in the syntax-worker. You cannot write a
  controller-level `KeyboardEvent` integration test without standing up a DOM
  harness. Coverage for the boundary is Rust-side + the CI wasm compile gate.
  Norn accepted this; flag it if you want to change it.

### Key files map

- `crates/iridium-editor/src/input/keyboard/` — directory module. `mod.rs`
  (dispatch + multi-cursor verbs + sticky/stack machinery), `editing.rs`
  (multi-cursor edit builders — `build_multi_cursor_command`, caret
  placement via byte offsets), `motions.rs`, `behaviors.rs` (tab stops,
  indent, auto-pairs, Enter expansion), `comments.rs`, `line_ops.rs`,
  `multi_cursor.rs` (occurrence scanning), `types.rs` (`KeyCode`,
  `Modifiers`, `KeyResult`), plus `*_tests.rs`.
- `crates/iridium-editor/src/editor/core.rs` — `EditorState`, `Editor`,
  `apply_command`, `refresh_search`, paste, language plumbing.
- `crates/iridium-editor/src/document/` — `buffer.rs` (`Document`,
  `revision`), `position.rs` (`Position::advanced_through` — the single
  source of truth for caret advance), `cursor.rs` (`CursorState`,
  `should_merge`).
- `crates/iridium-editor/src/search/` — `find.rs`, `replace.rs`
  (`ExpansionMode`, verify-or-skip).
- `crates/iridium-bindings/src/wasm.rs` — the `WebEditor` WASM surface.
  `key_map.rs`, `edit_tracking.rs` are supporting modules.
- `packages/@iridium/core/src/controller/index.ts` — the web controller
  (key translation, clipboard passthrough, IME guard, worker coordination).
- `packages/@iridium/syntax-worker/src/` — `worker.ts` (off-thread
  tree-sitter), `encoding.ts` (UTF-16↔UTF-8).

---

## 7. Open decisions — need Tom (do NOT decide these yourself)

From `PLAN.md §7`. These are parked awaiting Tom; several block downstream
work. Recommendations in the plan, summarized:

- **D1 — Keymap model** (modal / non-modal / layer). *The big one.* Blocks
  the skip-occurrence binding, macOS translation cleanup, and the terminal
  face's nvim feel. Plan recommends a keymap layer, non-modal default, modal
  as a keymap. **Must land before Phase 4 (terminal).** Tom has been asked
  repeatedly; still open.
- **D2 — Minimap:** wire it (it's finished/tested) or delete? Rec: wire.
- **D3 — napi target:** delete now, resurrect from the kernel later? Rec:
  delete now.
- **D4 — Package naming:** commit to `@iridium-editor/*`? (Half-done.)
- **D5 — Default web intelligence source:** local worker vs Meridian-served
  spans? (Worker currently default-off with a comment pointing at Meridian.)
- **D6 — Caret glide:** eased cursor animation on/off/absent? (Feel question.)
- **D7 — Cypher/SQL grammars:** back in scope (which phase?) or out?
- **D8 — Harness:** Tom's dispatch/Norn harness vs Claude workflows vs mixed.
- **Constitution amendment** (test-mod scoped lint allows) — still awaiting
  an explicit veto/blessing.
- **Web-IME relocation** — a proposal is drafted in `PLAN.md` Phase 2, marked
  "awaiting Tom." Full web IME doesn't work yet (the canvas isn't an editable
  element, so no composition events fire); the fix is the hidden-editable-
  element pattern. There's an `insertText()` seam ready for committed text.

---

## 8. Known warts / tech debt (logged, intentionally untouched)

- `crates/iridium-editor/src/syntax_stubs.rs` — two `unused_assignments`
  warnings on `in_line_comment`, visible only in the `--no-default-features`
  build. Pre-existing.
- `packages/@iridium/core/src/syntax/index.ts` — a dead `SyntaxHighlighter`
  class that still has the old UTF-16-as-bytes pattern but **zero callers**.
  Left alone because fixing it properly means relocating `encoding.ts` into
  core to share; flagged for whoever does the crate split.
- Broad pre-existing clippy backlog in render/view/mouse files (casts,
  unwraps). Being burned down per-phase via the ratchet, not all at once.

---

## 9. What's next: Phase 2 (NOT started — get Tom's go)

Structure work. From `PLAN.md`:
- **Crate split:** extract `iridium-core` as a GPU-free editing kernel (the
  CI already has a `--no-default-features` kernel gate as a precursor).
- **Hoist the compositor out of `wasm.rs`** (it's grown large).
- **Address perf cliffs.**

Phase order after: 3 syntax depth (injections, predicates, incremental
splice) → 4 terminal (`iridium-tui`, first host app — needs D1) → 5 desktop
(winit) → 6 LSP → 7 ship. Hygiene is a per-phase *exit criterion*, not a
final phase. Phases are internally parallelizable across agents; the phase
boundary is the quality gate.

**Performance targets:** 120fps, sub-8ms input latency. These are real; the
kernel/render split is partly in service of protecting them.

---

## 10. Memory

Persistent memory lives at
`/Users/tom/.claude/projects/-Users-tom-Developer-ablative-iridium/memory/`
with an index at `MEMORY.md`. Relevant files: `tom-working-style.md`,
`iridium-plan.md` (execution state — kept current), `ablative-stack.md`. Keep
`iridium-plan.md` updated as you land work; it's the cross-session baton.

---

### The one-paragraph version

Iridium is a Rams-minimal, AI-separable, terminal-first GPU text editor.
Phase 0 + all of Phase 1 are done and committed (18 commits, 538 editor
tests, clean tree) on `feature/lsp-hover-completion`. The working loop is:
implement (opus agents ok) → verify yourself → Norn xhigh adversarial review →
fix at root with regression tests → commit. Never defer silently — ask Tom.
Never economize on review. Respect the 500-line/zero-unwrap/zero-allow bar and
the banned ports. Phase 2 (crate split) is next but needs Tom's go, and D1
(keymap) is the decision gating the terminal face. Talk to Tom via Meridian.
