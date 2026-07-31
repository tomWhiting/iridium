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

**Test baseline, all green at `a86f59b`:** 649 (`--no-default-features`), 691
(`--no-default-features --features syntax`), 707 (`--all-features`), 92
bindings, 30 syntax, 3 host-extension integration tests. `cargo fmt --all --check` clean. Zero
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

## The registry workflow has LANDED

Commits `967f974` (registry + keymap layer) and `a86f59b` (size cap). Its final
`fix:findings` phase **died on a 529**, so I verified its two failing review
gates myself rather than trusting the report. Nearly every finding had already
been fixed by a later agent — `Editor::run_command` and `set_mode` exist,
`KeyResult::HostCommand` carries an unimplemented command to the host,
`CommandArgs` carries counts and captures, `FromStr for StrokePattern` parses
`ctrl+shift+k`, `check_cross_layer_shadowing` exists, `has_continuation`
respects cross-layer suppression, `resolve_repeat` handles auto-repeat,
`push_validated_keymap` exists, and `abort_pending_sequence` now has callers in
both `editor/core.rs` and `wasm.rs`.

**The one reported major that mattered — Ctrl+K silently eating the next typed
character — does not reproduce.** I wrote the repro as a real integration test
before touching anything: `step()` now retries a dead sequence from an empty
buffer, so the stroke falls through and self-inserts. There is a committed test
for exactly this (`a_dead_ended_sequence_replays_the_final_stroke_into_the_document`).
Lesson: an agent's review can be stale by the time you read it — reproduce
before you fix.

Two constitution violations in the generated code were still live and I fixed
them: `commands/stroke.rs` at 690 lines split into `stroke` / `stroke_text` /
`binding`, and `input/keyboard/dispatch_tests.rs` at 632 split its sequence and
user-layer tests into a child module. Then `input/keyboard/mod.rs` at 539 →
`keymap_api.rs` + 414.

### OPEN DECISION FOR TOM — live in the code right now

`Ctrl+K Ctrl+D` → `multiCursor.skipLastOccurrence` is the **only** binding in
the default keymap that is not a transcription of the old match statement. A
differential harness over 3,424 (KeyCode × modifier) combinations found exactly
four disagreements with pre-migration behaviour, all of them this.

It makes `Ctrl+K` a chord leader, so:

- Ctrl+K goes from `KeyResult::Ignored` to `Handled` at the host boundary — the
  web face now suppresses the browser default and forces a repaint where it
  previously passed the key through;
- on **macOS** `translateKeyEvent`
  (`packages/@iridium/core/src/controller/index.ts`) returns
  `ctrl: e.metaKey || e.ctrlKey`, so Cmd+K *and* Cocoa's Ctrl+K kill-line both
  arm the leader.

It no longer loses a character, and the pending sequence is now observable by
the host (`pendingKeySequence()` + a callback). Removal is one line from the
`BINDINGS` table plus `DEFAULT_KEYMAP_BINDING_COUNT` 51 → 50 and one relaxed
assertion in `every_registered_command_is_bound_except_the_typing_fall_through`.

The meta/ctrl conflation is **pre-existing and affects every ctrl binding**, not
just this one; it wants its own fix in the TypeScript translation layer, not a
rushed patch here.

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

**Unblocked as of `a86f59b`.** The registry landed, so the TUI's kernel
contract is now stable and better than it was: the terminal face maps
`termina::Event` → `terminput::Event` → iridium `KeyCode`/`Modifiers` and hands
it to the same resolver the web face uses, so both faces share one keymap by
construction. Verified stack facts are in `TERMINAL-STACK.md`.

## Soft wrap: designed, not yet implemented

`docs/SOFT-WRAP-DESIGN.md` is the output of a 16-agent design workflow (run
`wf_37d58d87-805`, all 16 agents succeeded), plus my own verification on top.
**Read its "Verified by hand" section first** — it corrects the map in two
places and records what is real versus latent.

Winner: **`new-vocabulary`** (108 points vs 94 and 91). Soft wrap gets a new
coordinate space, the **screen row**, in a new `crate::layout` module. The
existing fold "visual line" keeps its exact name, signature, meaning and
serialized shape, so the web face does not break. Rows compose directly from
document lines via `is_line_hidden` plus coalesced fold intervals, never through
`visual_to_document_line`. Widths are measured in **cells** (`unicode-width`
plus tab stops), which is the only unit that lets the terminal and GPU faces
agree on break points by construction — and it lets the GPU faces set
`Wrap::None` and delete the ad-hoc wrap model now living in `wasm.rs`.

**Every judge found a real flaw in the winning design. They are listed in the
doc and must be fixed before implementation.** The two that matter most:

1. The `RowIndex` invalidation hook fires on `Editor`'s fold mutators, but the
   **web face does not use `Editor`'s fold state** — `WasmEditor` owns a private
   `fold_state` (`wasm.rs:177`). The hook would never fire for the web face.
2. `RowIndex::apply_edit` takes a single contiguous line splice, but every
   multi-cursor edit and every paste hands it a `Command::Compound` with N
   edits in reverse order.

**First task of the implementation** is the latent `Viewport.first_line`
ambiguity (see the doc): reproduce-confirmed, currently unreachable because
every document-semantics method on `Viewport` is dead, and it must be made
impossible-to-misuse before anything new is built on it.

**Soft wrap is a consolidation, not a greenfield feature.** The web face already
wraps, ad hoc, in `wasm.rs`, invisibly to the kernel that owns the undo tree and
the cursor.

## (former) In flight: soft-wrap design workflow

**Workflow `wv59grbl1`** (run id `wf_37d58d87-805`), script at
`~/.claude/projects/-Users-tom-Developer-ablative-libs-iridium/33ce25a4-8b77-4d27-b58b-a132d9a104af/workflows/scripts/iridium-soft-wrap-design-wf_37d58d87-805.js`.
Resume with `Workflow({scriptPath, resumeFromRunId: "wf_37d58d87-805"})`.

**It is read-only** — map and design only, no implementation — so it does not
conflict with anything else in the checkout. Four parallel readers map the
viewport/fold/motion/consumer contract, then three designers argue three
angles (new vocabulary / unified mapping / lazy windowed), each judged by three
adversarial lenses. It returns a ranking, the winner, and the best ideas from
the losers.

**Review the design before anyone implements it.** The reason it is a design
workflow rather than an implementation one:

- The kernel already has a `visual line` concept and it means *folds* —
  `fold_state.rs` `document_to_visual_line` returns `Option<usize>`, a
  1:1-or-hidden mapping. Soft wrap makes the same relationship **1:N**, so the
  name is taken by a different meaning and the signature no longer fits.
- `document_to_visual_line` is **public API** re-exported through
  `iridium-bindings/src/editor.rs` to the web face. Changing its meaning while
  keeping its signature is the dangerous kind of change: it keeps compiling.
- `EditorConfig::word_wrap` (`editor/config.rs:89`) is **dead** — declared,
  defaulted to `false`, never read. That is the third dead config found in this
  crate, after `undo_group_timeout_ms` and its `set_content` sibling. Treat "a
  config field exists" as zero evidence that anything honours it.
- `render/viewport.rs` is 696 lines, already over the cap, and is pure layout
  with no GPU dependency — kernel code despite the directory name. It should be
  split as part of this work, not after.

## Decisions Tom has now given (2026-07-31)

Three questions this document was holding open are answered:

1. **What next → features, not the terminal face.** Build the things he uses
   daily, kernel-side, so they appear in the web demo immediately and the
   terminal face inherits them.
2. **`Ctrl+K` chord leader → drop it.** `Ctrl+K` becomes the command-palette
   key. The `Ctrl+K Ctrl+D` chord goes; `multiCursor.skipLastOccurrence`
   becomes palette-only.
3. **Undo tree → both.** Key bindings *and* a visual panel.

Still **not** given, and still must not be acted on: approval for the phase
reprioritisation in `THE-CORE-LOOP.md` §4. Do not touch PLAN.md's phase order
without it.

## The approved plan

`~/.claude/plans/immutable-stargazing-moler.md` — approved 2026-07-31. Order of
work: **command palette → text transformations → undo-tree keys and panel →
syntax-node navigation.** Soft wrap and the terminal face are explicitly out of
this stint (the soft-wrap design notes above stay valid for whenever it starts).

Three findings in that plan were verified by hand and are worth repeating here,
because each contradicts a reasonable assumption:

- **The web face's Rust has tree-sitter switched off.** `wasm.rs:27` imports
  `syntax_stubs`; the real tree-sitter lives in a JS worker behind a span-only
  protocol. Kernel AST navigation will not appear in the browser without a
  further decision (plan §4.5 prices the three options).
- **`onHostCommand` was dead in TypeScript.** Declared, read, never assigned in
  the constructor — so every host command the kernel resolved was silently
  dropped. Fixed; see below.
- **The kernel's folds go stale after every edit.** `update_regions` is called
  only from `set_content` and `set_language`, never from
  `apply_command_internal`. The web face drives its own `fold_state` so it is
  unaffected; this bites the terminal face and any native embedder. Plan §4.6
  step 4 fixes it.

## Progress against the plan

Palette build order, steps 1–11 (plan §1.7). **Steps 1–7 are done and
committed** — the whole kernel half. Commits, oldest first: `ce3c715`,
`dd09cb3`, `6c8e592`, `ea1feb5`, `8e6b097`, `7c17dec`, `695bac0`, `f763edf`,
`646d4b0`, `f74bf53`.

1. ✅ **`onHostCommand`/`onPendingKeySequence` now assigned**
   (`controller/index.ts`). The stored-callback type changed from
   `Pick<…>` — which keeps properties optional and so permitted the omission —
   to a mapped `HostCallbacks` type that *requires* every key. Verified the
   guard discriminates: removing the assignment again is now a compile error.
2. ✅ **`KeymapStack::binding_is_reachable`** (`commands/stack.rs`) plus
   `commands/reachability_tests.rs` (13 tests).

   The method answers "does pressing this binding's own keys run this binding?"
   by synthesizing the minimal keypress for each stroke and asking the resolver,
   rather than re-deriving the precedence rules. **It must check prefixes as well
   as the whole sequence**: `KeymapResolver::resolve` tests `exact_match` at
   `resolver.rs:357` *before* `has_continuation` at `:383`, so a complete binding
   on `Ctrl+K` fires immediately and `Ctrl+K Ctrl+D` never gets its second
   keystroke. The first implementation compared whole sequences only and reported
   the stranded chord as reachable; the test
   `a_leader_bound_in_a_higher_layer_strands_the_chord_below_it` caught it.

   Wildcard (`AnyChar`) bindings need a witness *character*, and a literal
   binding always outranks a wildcard on the character it claims, so candidates
   are tried until one resolves — a single fixed character would report a live
   wildcard as dead.
3. ✅ **`commands/hints/`** — `KeyHint`, `KeyHintIndex`, `KeyLabelStyle`, 17
   tests. The load-bearing test is the **oracle**: every hint, typed as its label
   describes, resolves back to the binding it came from, which covers every
   present and future way a binding can be lost in one assertion.

   Labels are rendered *separately* from `display_sequence()`, which is the
   round-trippable form and spells ignored modifiers as `~name` — *Select All*
   round-trips as `ctrl+~shift+~altgraph+a`. Labels emit only `Required`
   modifiers, giving `Ctrl+A`. Verified discriminating: re-emitting `Any`
   modifiers fails three tests.
4. ✅ **`KeyHintIndex` cached on `KeyboardHandler`**, `Editor::key_hints()`
   exposed. All four keymap mutators funnel through one private
   `install_keymap`, and the field is module-private, so no mutation can skip the
   rebuild.

   **A test-quality lesson worth keeping:** the first staleness test drove the
   *editor*, whose `push_keymap` routes through `push_validated_keymap` — so raw
   `push_keymap` and `set_keymap` were never exercised despite the test name
   claiming every mutator. Fixed with a handler-level test reaching all four,
   verified by breaking each mutator in turn.

5. ✅ **`CommandMeta::aliases`** (`f763edf`) — `&'static [&'static str]` with a
   `const fn with_aliases`, applied to **28** built-ins (the plan said "roughly
   15"; every one added contributes a term the title, id, category and
   description all miss). Aliases are search terms only — nothing *resolves* by
   alias, so one can never change what a key runs.

   **Aliases are write-only over serde**, deliberately. `skip_deserializing`
   would make `"aliases": ["fmt"]` in a host manifest silently do nothing, and a
   synonym that never matches cannot be diagnosed from the outside; the field has
   a deserializer that rejects a non-empty list, naming `with_aliases`. An
   explicitly empty list still round-trips.

   The same synonym on two commands is legitimate (`erase` on both deletions), so
   the table is **not** globally deduplicated — anchored by a test, because a
   well-meaning uniqueness check would break it.
6. ✅ **`commands/palette/`** (`646d4b0`) — `matcher.rs`, `entry.rs`, `mru.rs`,
   39 tests. Integer arithmetic over **character** positions: no floats (identical
   ranking in every face by construction), no recursion (linear, not exponential),
   no allocation (positions cannot outnumber a query capped at 32).

   Fields are scored independently — title 100, alias 90, id tail 85, category 60,
   description 50 — and the winner reports *itself* plus the exact text the
   positions index, so a palette highlights the alias it matched rather than
   underlining the title.

   Assignment runs **two** linear passes, leftmost and rightmost, keeping the
   better: forward alone is the canonical subsequence test but scores badly (`li`
   against "Duplicate Line" takes the `l` of *Duplicate*, not the word-initial
   `L`).

   Recency: 16 entries, `+400 − 20×rank`. The scale is the point and is tested in
   **both** directions — strong enough to decide between comparable matches, weak
   enough that typing a command's own title still finds it with a loaded history.

   Four deliberate breaks (aliases unscored, byte offsets, no backward pass, no
   recency) each failed exactly the tests that claim them.
7. ✅ **`palette.open` registered and bound** (`f74bf53`, plus `8e6b097` earlier
   for the `Ctrl+K Ctrl+D` removal). New `commands/builtin/host.rs`: commands the
   kernel *names* but does not implement. It stays out of `BUILTIN` — the action
   table is exhaustively matched, so an entry there would be a compile error
   demanding an implementation the kernel cannot write. **`default_registry()`**
   is the union and is what `Editor::new` seeds.

   Bindings: `Ctrl+K` (Shift **forbidden**, so it cannot swallow the
   `Ctrl+Shift+K` that deletes a line) and `Ctrl+P` (Shift `Any`, so one binding
   serves `Ctrl+P` and `Ctrl+Shift+P`). `DEFAULT_KEYMAP_BINDING_COUNT` is now
   **52**.

   **This broke 18 tests, and the pattern is worth remembering**: every test that
   hung a chord on `Ctrl+K` did so *because the default left it free*. They now
   use a `CHORD_LEADER` constant on `Ctrl+B`, named once per file — this is the
   second time that coupling has broken these tests, and the constant is what
   stops a third.

   Two checks were re-shaped rather than loosened.
   `every_default_binding_names_a_command_the_kernel_or_a_host_owns` now
   *separates* the cases: a host command must have no kernel implementation,
   anything else must have one. Verified a typo'd id still fails eight tests.
   `every_command_the_palette_lists…` asserts host commands return exactly
   `Unimplemented` naming the id, and that kernel commands never do.

   **Gotcha found here:** `Keymap::suppresses` matches whole sequences
   *including modifier patterns*, so a host unbinding the default's `Ctrl+K` must
   spell it identically — `AltGraph: Any` and all — not with the tighter
   `NONE.with_ctrl(Required)`. Pinned in
   `a_host_may_take_ctrl_k_back_by_unbinding_it_first`.

Test counts now: **789** all-features (was 710 at plan approval), **731**
GPU-free, **773** syntax-without-GPU, 92 bindings, 30 syntax. `cargo fmt --check`
clean, wasm target compiles, no new clippy warnings.

8. ✅ **`crates/iridium-bindings/src/palette.rs`** + four wasm exports
   (`af5858b`), 18 tests. Deliberately **not** gated on `feature = "web"` — plain
   functions over borrowed kernel types, so `cargo test` covers them on the host
   target and only the adapters are browser-only. A conversion bug reproducible
   only in a browser is one nobody reproduces.

   **Match offsets are converted to UTF-16 at this boundary.** The kernel indexes
   by *character*, which is right for Rust and wrong for a browser — one astral
   character in a title shifts every highlight after it. Pinned with `𝄞`
   (one `char`, two UTF-16 units) and `ü` (two bytes, one unit), so a conversion
   written against either characters *or* bytes fails.

   `matchedText` is on the wire beyond the planned shape, and has to be: an alias
   hit cannot be highlighted inside the title.

   `WebEditor::consume_key_result` extracted; `runCommand` goes through it, so a
   palette invocation and a keypress are indistinguishable downstream. An
   unimplemented id is stashed for `takePendingHostCommand` and reported as
   `handled:command`, leaving `ignored` to mean only "no such command".

   **`CommandMru` lives on `WebEditor`**, beside the `KeyboardHandler` this face
   actually routes through — *not* inside `self.editor`, whose own handler this
   face never drives. (This was the open decision; it is now taken.)
9. ✅ **TS controller surface** (`af4089e`) — `listCommands`, `searchCommands`,
   `runCommand`, `keyHintFor`, `blurEditor`, `usesMacKeyLabels`, plus the
   `PaletteCommand` type. `applyActionOutcome` extracted from `handleKeyDown` for
   the same reason as the Rust extraction.
10. ✅ **`@iridium/core/src/palette/`** (`af4089e`) — the framework-free state
    machine, 26 bun tests, plus `"./palette"` in both `deno.json` and
    `package.json` exports. Decisions pinned by tests: **clamp, never wrap**
    (wrapping overshoots under key repeat); focus returns to the editor on close;
    a new query resets the selection; no debounce; close *before* running;
    an unavailable command neither runs nor dismisses.

Test counts now: **789** all-features, **731** GPU-free, **773**
syntax-without-GPU, **110** bindings, 30 syntax, **43** bun (26 new). `cargo fmt
--check` clean, wasm target compiles, `deno check` clean, no new clippy warnings.

11. ✅ **Both web faces draw the palette** — `4f6edf9`, and it is **not on
    `main`**: see "Step 11 is parked in a worktree" below. `CommandPalette.tsx`
    (React, portalled to `document.body`), `element/palette.ts` (vanilla DOM,
    inside the shadow root), `Iridium.tsx` gains `onHostCommand`, `App.tsx` and
    the web component wire `palette.open` → `CommandPalette.open()`.

    **Match highlighting is shared code, not per-face** — new
    `@iridium/core/src/palette/highlight.ts`, 17 tests. The kernel's offsets are
    UTF-16 and point at where each matched *character* starts, so a run is not
    one code unit wide: an astral character spans two, and the obvious
    "one offset, one unit" loop splits the surrogate pair. The failure is silent
    — a lone surrogate is a valid JS string that renders as a replacement glyph
    — which is exactly why it is not left to each face to rediscover. Verified
    against three deliberate breaks (naive one-unit runs, no coalescing, clamping
    instead of dropping out-of-range offsets).

    **The web component cannot portal to `document.body`** — a shadow root's CSS
    does not reach outside it. The overlay mounts *inside* the root with
    `position: fixed`. It carries its own `<style>` element rather than being
    appended to the template literal in `element/index.ts` as the plan said; same
    effect, and it keeps that file inside the size cap (321 lines).

    `Iridium.tsx` routes **all three** callbacks through a ref. The editor
    captures its callbacks once at construction while props change every render,
    so a callback closing over component state would fire against the first
    render's snapshot. Latent today; a real bug the moment anyone uses it.

    Both faces build their `PaletteHost` to delegate through the editor ref
    rather than capture an editor, so the palette works from the first render and
    lists nothing until wasm is ready.

    Also fixed a **latent vite alias bug**: a string alias matches `find` *or*
    anything under `find/`, first match wins, so the bare `"@iridium/core"` entry
    listed ahead of `"@iridium/core/syntax"` and `"/element"` was already
    shadowing them. Longest prefix now comes first.

    **Found and fixed in a self-review afterwards: `Ctrl+K` killed the query it
    opened.** macOS binds `Ctrl+K` in a text field to kill-to-end-of-line, and
    `Ctrl+K` is the palette's own open key — so pressing it again while open
    deleted the rest of what the user had typed and looked like nothing had
    happened. It now closes. `Meta` counts alongside `Control` throughout,
    because the web face forwards macOS `Cmd` as the kernel's `ctrl`, so `Cmd+K`
    opens the palette and must close it too.

    That fix moved key handling into `palette/keys.ts` as a pure function over a
    key description, called by **both** faces — it had been a duplicated switch
    in each, which is precisely how they would have drifted, and being pure makes
    it testable with no DOM, which the views are not. 12 tests, verified against
    four deliberate breaks. **The views themselves still have no test harness**
    at all; that is why anything behavioural is pushed out of them.

Gates run for step 11: `deno check` clean across the whole core package,
**55 bun tests** (29 new), `npx tsc --noEmit` clean on `examples/web`, and
`npx vite build` succeeds. No Rust changed, so the cargo baselines above stand.

## Step 11 has LANDED — merged, rebuilt, server restarted (31 Jul, ~18:15)

Tom gave the go-ahead ("you're totally fine to restart and rerun stuff"), so the
whole thing is now on **`feature/terminal-face`**, merge commit `d5ec40b`. The
worktree branch `feature/palette-web-face` is merged and can be deleted along
with `<scratchpad>/step11-wt`.

What was done, in the order it had to happen:

1. **Wasm rebuilt first** — `wasm-pack build crates/iridium-bindings --target web
   --features web --no-default-features`. Merging first would have broken the
   running demo, because the merged UI calls exports the old bundle lacked. Took
   **17s**; the target was warm.
2. **Merged** the palette branch. Clean, no conflicts.
3. **Dev server restarted** on 12223 (the old pid 17942 was serving a module
   graph with none of this in it).

**Verified after the restart:** all four palette exports are in the new bundle
(`listCommands`, `searchCommands`, `runCommand`, `keyHintFor` — previously 0
matches), their `.d.ts` signatures match the TS `WebEditor` interface exactly,
vite serves `palette/{index,keys,highlight}.ts` at 200, and `App.tsx` resolves
`@iridium/core/palette` to the palette module rather than to a path underneath
`controller/index.ts` — which confirms the alias-ordering fix was load-bearing,
not cosmetic. Gates: **55 bun tests**, `deno check` clean, `tsc --noEmit` clean.

**Operational trap, learned the hard way: never `git switch` under a live vite
server.** Landing this on `main` I ran `git switch main` *before* the
fast-forward. At that instant `main` was still at `4f73090`, which has none of
the palette files — so git deleted them from the working tree, the watching vite
server cached the resolution failure for `@iridium/core/palette`, and restoring
them two seconds later by merging did **not** invalidate that cache. The demo
served a 500 until the server was restarted and `node_modules/.vite` cleared.

Do it the other way round: merge into the branch you are on, or move the ref
without touching the working tree (`git push origin <branch>:main`,
`git branch -f`). The working tree under 12223 should never transiently lose
files.

**Second operational trap: do not start the demo server as a harness-tracked
background task.** A server started that way is owned by the task runner and gets
killed when the task is cleaned up — which happened mid-session while Tom was
using it. Start it detached instead, so it outlives the session that started it:

```bash
cd examples/web && nohup npm run dev > <scratchpad>/vite-12223.log 2>&1 &
```

**Why the demo could never have worked before this**, since it came up: the
served bundle was built at 10:04 and the palette exports landed at 14:17, *and*
the UI files were on a branch, not in the checkout. Neither half was present, so
`Ctrl+K` did nothing. Not a version fluke.

**The outstanding end-to-end checks**, once rebuilt: `Ctrl+K` opens the palette,
arrows clamp at both ends, `Enter` runs, `Escape` restores focus to the canvas,
and every entry shows the key that runs it.

After that, the plan's sections 2–4 (text transformations → undo-tree keys and
panel → syntax-node navigation).

## Multi-cursor was INVISIBLE in the web face — fixed 1 Aug (`f85d4a0`)

Reported by Tom as "Add Cursor Above/Below isn't wired up" from the new palette.
It was not the palette, and not the kernel.

**`wasm.rs` drew one caret and one selection highlight, both from
`cursor.primary`.** `all_selections` and `secondary` appeared nowhere in the
file. The kernel created N selections correctly and reported them correctly; the
face showed one. A command that worked perfectly looked like one that did
nothing.

Diagnosis order that got there, worth repeating: confirmed both ids registered
*and* in the ACTIONS table, then wrote a kernel test running the command **by
name** and **by key** over the same document and comparing every caret. They
agreed — which is what ruled out the whole kernel and the palette in one step and
pointed at the renderer. That test is committed
(`adding_a_cursor_vertically_by_id_matches_the_key`).

**The fix:** both quad paths walk `all_selections()`. The primary's position is
still computed separately — `ensure_cursor_visible` scrolls to it alone and
inline blame sits on its line — but its *caret* comes from the same loop, so no
primary-shaped special case is left to drift. Blink stays shared on purpose.

**`cursorCount()` was added and wired to the demo's status bar**, and that is the
part that matters beyond this bug: every export on this face reports the primary
and none published a count, so nothing outside the kernel could contradict the
renderer. A visible count makes the class of defect loud.

**Nothing tests this.** `wasm.rs` is `cfg(target_arch = "wasm32")` and the render
path is GPU-coupled, so no native build compiles it — exactly how this survived.

## The `.primary` audit — DONE, and it found a second reachable bug (`ce6add9`)

The audit the entry above demanded. Every `.primary` in `wasm.rs` was treated as
suspect; most deserved it.

**The reachable one: macOS `Cmd+Backspace` / `Cmd+Delete` destroyed
multi-cursor.** `deleteToLineStart` / `deleteToLineEnd` were implemented by hand
in `wasm.rs` against the primary caret, and finished with
`Editor::set_cursor` — whose own doc says it *replaces all cursors with a single
one*. Four carets, one keystroke: three lines spared and three carets gone. The
TS controller reaches these from its own `Cmd` branch
(`controller/index.ts` ~:732), before `translateKeyEvent`, so this is live on the
platform Tom uses.

The same shape ran through the whole `extendSelection*` family — eleven methods
computing a motion against `cursor.primary` and handing it to
`Editor::set_selection`, which is documented as replacing all cursors with one
selection.

**Two verbs the kernel did not have.** `edit.deleteToLineStart` and
`edit.deleteToLineEnd` are now kernel commands built through
`build_multi_cursor_command`, so they are multi-cursor by construction, land in
the palette, and the terminal face inherits them. Critically they are now
compiled by `cargo test` — nothing compiled the old code on any native target,
which is exactly how it survived.

They are **deliberately unbound** in the default keymap (the exception list in
`every_registered_command_is_bound_except_the_typing_fall_through` is now five
entries and says why): `Ctrl+Backspace` / `Ctrl+Delete` are already word-wise
delete, and that keymap is platform-neutral. **Whether they should get keys is
Tom's call and is open.**

**Eleven methods deleted rather than rewritten.** `extendSelection*`, `selectAll`
and `clearSelection` route to the kernel command the identical keystroke already
runs. They *did* disagree: `extendSelectionLineStart` went to column zero while
`Shift+Home` does smart home. `hasSelection` now answers for any caret;
`getSelectedText` joins every selection the way `copyText` does. The four
`getSelection*Line/Column` accessors stay primary-only **on purpose** and now say
so — they are singular questions, and `cursorCount` is how a host learns there
are more.

`wasm.rs` 3,969 → 3,898. Still far over cap, but the direction is right and the
duplicated motion layer is gone.

`editing.rs` was already over cap at 605 and this pushed it to 654, so it split
into `editing/{mod,intents}.rs` along the seam already there: command
construction versus the per-cursor edit intents it consumes.

**11 tests in `line_boundary_tests.rs`**, verified against four deliberate
breaks. One useful negative result recorded in the test itself: measuring the
line end in *bytes* does **not** fail any test, because `clamp_edit_ranges` pulls
an over-long column back to the real line end. The test says so rather than
claiming a discrimination it does not have.

**Still primary-only, and correctly so:** `setCursorFromClick`,
`startSelectionAt`, `extendSelectionToPosition` — a click and a drag are
single-caret gestures by definition.

**Still primary-only and NOT yet fixed:** `delete_selection` (the private helper
behind the `backspace` / `deleteForward` exports). Its only caller today is
`applyCompletion`, which is single-caret by nature, so it is not reachable from
typing — but it is a public wasm export and it is wrong. Named here so it is not
lost.

## Section 2 (text transformations) — DONE (`cfbc11c`, `f356cf1`)

Fifteen `transform.*` verbs, in the palette and in the wasm bundle serving
12223. Built in two commits on purpose: the pure core first, then the verbs.

**`crates/iridium-editor/src/text/`** — `case.rs` and `lines.rs`, pure `&str`
functions with no idea what a document or a cursor is. Owned rather than `heck`
or `convert_case`: the content this editor serves is JSON and Markdown, so the
input is routinely neither ASCII nor a well-formed identifier, and how
`HTTPResponse`, `v2Beta`, `café-au-lait` and `sha256` behave is worth pinning
here rather than inheriting. 21 tests, eight breaks.

The three decisions worth not relitigating:

- **The acronym rule.** A run of uppercase followed by uppercase-then-lowercase
  starts the new word at the *last* uppercase, so `HTTPResponse` is
  `HTTP` + `Response`, not `HTTPR` + `esponse`.
- **Digits are word material** and never start a word alone. Splitting on digits
  would shred every version string in a JSON file.
- **Sorting is Unicode scalar order, not locale collation.** A sort whose answer
  depends on the machine's locale makes "same document, same command, different
  result" possible, which is the one thing this architecture exists to prevent.
  The line ending is likewise a *parameter*, never sniffed — sniffing per call
  is how a CRLF file ends up with both.

**`input/keyboard/transform.rs`** — the part that only exists once a verb meets
a document. Case verbs act on each caret's selection, or the word under it;
line verbs expand to touched lines and merge overlapping **and adjacent**
blocks (two selections on lines 0-1 and 2-3 share no line but do share the
boundary between them). 23 tests, seven breaks.

**All fifteen are palette-only.** Approved by the plan, recorded in the
`every_registered_command_is_bound_except_the_typing_fall_through` exception
list, and **raised with Tom** — which three or four earn real keys is his call
and is still open. That test's `unbound` vec is now 20 entries and is
*ordered by registry order*; the `- 3` is now `- 20`.

Two files crossed the 500-line cap and were split along seams already there:
`editing.rs` → `editing/{mod,intents}.rs` (command construction vs the
per-cursor edit intents it consumes), `actions.rs` → `actions/{mod,run}.rs`
(the declarative enum-and-id-table, which is what you read, vs the routing
`match`, which is what you edit).

Counts now: **845** all-features, **787** GPU-free, **829** syntax-without-GPU,
110 bindings, 30 syntax, 55 bun.

## Section 3 (undo tree) — kernel + wasm DONE (`b547c00`), PANEL NOT STARTED

### What is done

**The reachable bug found on the way in: the palette's *Undo* did nothing.**
`history.undo` resolved to `KeyResult::Handled` — a bare acknowledgement — and
each face undid by its own private route. The web face intercepted `Ctrl+Z` in
`handleKeyEvent` **before the keymap**. So `Ctrl+Z` worked and nothing else about
undo did: the palette ran the command and the command did nothing, and no keymap
layer could rebind undo because the key never reached one. Same class as the
multi-cursor bug and the `deleteToLineStart` bug — *a face reimplementing a verb
outside the kernel*. That is three in a row; treat it as the standing suspicion.

Fixed with **`KeyResult::History(HistoryRequest)`** — `Undo`, `Redo`,
`RedoBranch(usize)`, `NextBranch`, `PreviousBranch`. The handler names the
request; `Editor::consume_key_result` performs it via
`Editor::perform_history_request`. Both `handle_key` and `run_command` funnel
through that, so key and palette are one path by construction. The web
interception is **deleted**.

**`Ctrl+Shift+Z` now redoes.** It undid — the registry migration transcribed a
dispatch that matched `'z'` regardless of `Shift`. `DEFAULT_KEYMAP_BINDING_COUNT`
is now **55**.

**New bindings:** `Ctrl+Alt+Z` → `history.previousBranch`, `Ctrl+Alt+Y` →
`history.nextBranch`. `history.redoBranch` takes a *count* so it has no bare
chord; palette-only, and it is what the panel will call by id. The `unbound`
exception vec is now **21** entries and the count assertion is `- 21`.

**New kernel API:** `UndoTree::snapshot() -> UndoTreeSnapshot` (whole tree, one
call — the tree changes every keystroke and node-by-node would be N boundary
crossings per repaint), `UndoTree::cycle_branch(forward)`,
`UndoTree::active_branch_index()`, `Editor::history_snapshot()`,
`Editor::cycle_history_branch()`, `EditorEvent::HistoryBranchChanged` (the only
signal a panel has that its highlight is stale — a cycle applies nothing, so
neither ContentChanged nor SelectionChanged fires).

**New wasm exports:** `historySnapshot()`, `jumpToHistoryNode(id)`,
`redoBranch(index)`. Ids are decimal **strings** (u64 vs JS number). All three
share `with_whole_document_edit` with `undo`/`redo` so no traversal can forget
the conservative edit-span record.

`CommandContext::history` was removed (dead once undo/redo stopped reading it);
the parameter stays as `_history` in the signatures.

### What is NOT done — this is where to pick up

1. ~~**No new tests were written for any of section 3.**~~ **CLOSED by
   `b812f02`.** Eleven tests — 7 in a new
   `history/undo_tree/branch_tests.rs`, 4 appended to
   `editor/history_nav/tests.rs` — against twenty deliberate breaks, every
   break caught and every test the unique catcher of at least one. Two things
   worth keeping from doing it:
   - **A two-way fork cannot distinguish `nextBranch` from `previousBranch`**,
     because forward and backward land on the same branch. The first version
     of both editor-level tests used one and passed with the two verbs wired to
     each other. Every branch test here now forks **three** ways; the helper
     `editor_forked_three_ways` says so in its doc comment. Apply the same rule
     to the panel's key handling when it lands.
   - `active_branch_index` is asserted **against `redo` itself**, not against a
     restatement of its rule, because the agreement of those two is the only
     thing the method is for.
   Totals moved to **856** all-features, **798** GPU-free, **840**
   syntax-without-GPU.
2. ~~**The wasm bundle has NOT been rebuilt**~~ **CLOSED.** Rebuilt twice (once
   for `b547c00`, again after the rename below) and verified served: `curl` the
   aliased `pkg/iridium_bindings.js` off 12223 and it contains `historySnapshot`
   and `childIds`. The command is
   `wasm-pack build crates/iridium-bindings --target web --features web
   --no-default-features`, run from the repo root — vite has `pkg` in
   `optimizeDeps.exclude` and aliases it to the real path, so a rebuild reaches
   the browser on reload with no server restart.
3. ~~**The TypeScript surface is untouched.**~~ **CLOSED by `cb5758f`.**
   `redoBranch`, `jumpToHistoryNode` and `historySnapshot` are on the
   `WebEditor` interface and on `IridiumEditor`, with `UndoTreeSnapshot`,
   `UndoTreeInfo` and `UndoTreeNode` exported as types.

   **One decision taken while doing it, worth knowing before the panel:**
   `UndoNodeInfo`/`UndoTreeInfo` serialized in **snake_case** (plain serde over
   Rust field names) while the palette's wire shape is **camelCase**. Both cross
   the same boundary into the same language. Nothing consumed either type yet,
   so they now carry `#[serde(rename_all = "camelCase")]` and there is one
   convention over the boundary. `the_snapshot_serializes_in_the_shape_a_host_reads`
   in `branch_tests.rs` pins the JSON key by key and fails if the rename goes.
   **Ids cross as decimal strings** — `u64` in the kernel, and a JavaScript
   number would round them. Never compare them arithmetically.
4. ~~**The panel itself is not started.**~~ **CLOSED.** `5be331a` names
   `history.togglePanel` as a host command beside `palette.open` and binds it to
   **`Ctrl+Alt+H`**, joining `Ctrl+Alt+Z`/`Ctrl+Alt+Y` so the whole undo-tree
   family is one chord shape. `d4c75b7` is the framework-free behaviour
   (`@iridium/core/history`: `UndoTreePanel`, `buildRows`, `formatAge`, the key
   table), `8922a9f` the React overlay, `432493c` the web component's.

   **Section 3 of the plan is complete.**

   Four design decisions inside it, none of which the plan settled:
   - **The arrows read as a tree, not a list.** Up is the *parent*, not the row
     above — the row above may be a sibling, and stepping into a sibling's
     subtree when the user asked to go back is the confusion a tree view exists
     to avoid. Down follows the *preferred* child.
   - **Browsing never touches the document.** ←/→ move only the selection. The
     plan says "←/→ switch branch", which could have meant driving
     `history.nextBranch`; it does not, because looking down a branch has to be
     free or looking is itself an edit. Enter is what commits.
   - **The selection is a node id, not a row index**, because a new branch above
     the selection shifts every index and the tree is re-read on every change.
   - **The active path is followed down from the root**, not up from the current
     node, so everything below the cursor — the user's own future — draws as
     live rather than as abandoned.

   49 bun tests across the three files, against **thirteen** deliberate breaks,
   every one caught. Two are worth remembering: a two-way fork cannot tell ←
   from →, so every fixture forks three ways; and the layout walk carries a
   cycle guard, because a malformed snapshot cannot come from the kernel but its
   failure mode here is a hung renderer.

   **Not verified by hand in a browser.** The bundle is current and 12223 serves
   every new module (checked by `curl`), but nobody has actually pressed
   `Ctrl+Alt+H`, forked the history, and jumped to an abandoned branch. That is
   plan §Verification step 3 and it is still owed.

## Section 2 — the pre-flight facts, re-verified 31 Jul (kept for reference)

Checked by hand against the tree as it stands, because the palette work moved
several of these files. **Still exact:** `KeyboardAction` at
`input/keyboard/actions.rs:78`, the `ACTIONS` table at `:215`,
`build_multi_cursor_command_placed` at `editing.rs:177`, `line_ops::join_lines`
at `line_ops.rs:531`. Neither `heck` nor `convert_case` is a dependency, so the
plan's "write the ~120 lines of word-splitting" stands.

Two corrections to the plan's text:

- **`CommandCategory` constants live in `commands/names.rs`** (`NAVIGATION`
  through `GENERAL`), not in the builtin module. `TRANSFORM` goes there.
- **`every_registered_command_is_bound_except_the_typing_fall_through` is now at
  `default_keymap_tests.rs:157`**, not `:101-129`.

**The trap in that test**, which the plan does not mention: it asserts
`assert_eq!(unbound, vec![...])` — an **ordered, exact** list, currently
`[EDIT_INSERT_CHARACTER, MULTI_CURSOR_SKIP_LAST_OCCURRENCE, COMMAND_NO_OP]` in
`registry.commands()` iteration order — and separately
`assert_eq!(bound.len(), BUILTIN_COMMAND_COUNT + HOST_COMMAND_COUNT - 3)`. So
landing N deliberately palette-only transforms means extending that vec **in
registry order** and changing the `- 3` to `- (3 + N)`. Getting the order wrong
fails with a diff that looks like a missing binding rather than a sorting
problem.

**Still needs Tom** before section 2 finishes: *which* three or four transform
verbs get real bindings. The plan recommends binding only those and marking the
rest palette-only, which is approved — but it does not say which, and that is a
question about his daily use, not a technical one. The pure case-conversion core
can be written and tested without answering it.

**Unresolved and NOT silently deferred:** registry-level validation of
*host-registered* aliases (empty, whitespace, colliding with a command id). The
built-in table is covered by tests; a host registering garbage aliases today only
degrades its own palette. Adding `RegistryError` variants is a public API change
that is not in the approved plan, so it is Tom's call.

## Demo state (2026-07-31, visitors)

The vite dev server on **12223** is being shown to guests today. Standing
instruction from that thread: **nobody rebuilds, pulls, or restarts it.** The
wasm bundle was rebuilt at 10:04 with the `Ctrl+K` fix in it, verified serving,
and the demo typechecks against it.

**This instruction is why step 11 is in a worktree** (see above). It also
constrains anyone picking this up: vite hot-reloads on save, so *any* edit under
`examples/web/src/`, `packages/@iridium/core/src/` or `examples/web/vite.config.ts`
reaches the guests' browsers immediately. Rust and `docs/` are safe — neither is
in vite's module graph — but a `cargo build` is CPU-heavy enough to be worth
avoiding while the demo is on screen.

Known rough edges the demo has, told to the demo runner so they steer around
them rather than discover them live:

- **`Ctrl+F` is silently inert** — it resolves to a search-open request and the
  demo never wires `onSearchAction`, so nothing happens at all. Search works in
  the kernel; the demo has no search UI.
- ~~**`Ctrl+Shift+Z` is undo, not redo**~~ — **fixed in `b547c00`.**
  `Ctrl+Shift+Z` redoes; `Ctrl+Y` still does too.
- ~~**The branching undo tree is not demonstrable in the browser.**~~ — **fixed.**
  `Ctrl+Alt+H` opens the panel, `Ctrl+Alt+Z`/`Ctrl+Alt+Y` point redo at a
  different fork, and `historySnapshot`/`jumpToHistoryNode`/`redoBranch` are
  exported. Still unpressed by a human, so demonstrate it privately once before
  putting it in front of anyone.
