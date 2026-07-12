# Iridium — The Plan

**Created:** 2026-07-12
**Status:** Draft for review
**Supersedes nothing** — this sits alongside `VISION.md` (the why) as the what-and-when.

---

## 1. Identity

Iridium is a text editor in the Dieter Rams sense: *Weniger, aber besser*. A
toaster that toasts, forever. The soul of the product is:

- **Instantaneous feel.** Keystroke-to-glyph so fast it feels precognitive.
  Everything else is negotiable; this is not.
- **Tree-sitter as a first-class organ**, not a bolt-on: highlighting,
  injections, folds, structural selection, text objects.
- **Text transformations and multi-cursor as the core verbs.** Selections,
  motions, simultaneous edits — the things you do to text, done perfectly.
- **AI-separable by construction.** No AI inside the editor. The editor
  exposes a complete, honest API (content, selections, transformations,
  events, position↔pixel mapping); AI hosts (Norn, Meridian) attach through
  the front door like any other host. The editor must be excellent *alone*.
- **One kernel, three faces:** terminal, desktop, web. Same editing engine,
  same feel, thin presentation shells.

Within the ablative stack: Iridium is the editing component consumed by
Meridian (web), Frame (app framework), and a standalone terminal binary.
Norn's TUI is a sibling consumer precedent, not a dependency.

### Non-goals (permanent)
No plugin system. No file trees, tabs, splits, or project management in the
core. No AI features in the core. No rich-text/WYSIWYG. Hosts do hosting;
Iridium edits text.

---

## 2. Quality Constitution (non-negotiable, enforced per phase)

Every phase's exit criteria include leaving **every module it touched** at
this standard. The final phase flips global CI enforcement; nothing waits
for it.

1. **Zero clippy warnings** at `--all-features --all-targets`, pedantic +
   nursery, enforced with `-D warnings` in CI.
2. **Zero `#[allow]` in non-test code** — current count 60. Each one is
   either fixed or the underlying design corrected (e.g.
   `too_many_arguments` → context structs; `dead_code` → wire the feature or
   remove it, per §6 dispositions). *Sole sanctioned exception:* a test
   module may carry one scoped
   `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`
   (or the subset it needs) directly on `mod tests` — unwrap/panic is the
   assertion idiom of tests, and the scoped allow is what lets the
   workspace-wide `-D warnings` ratchet coexist with idiomatic tests.
   [Amended 2026-07-12 during wave 1; flagged to Tom for veto.]
3. **Zero `unwrap()`/`expect()` in non-test code** — current count ~20.
   Errors propagate or are handled; no panics in library paths.
4. **500-line module cap** — current violations: 15 Rust files (worst:
   `wasm.rs` 2,983; `keyboard.rs` 1,623; `editor/core.rs` 1,277), 1
   non-generated TS file. Splits happen as modules are restructured in their
   phase — no big-bang lint pass on code about to be dismembered.
5. **Unsafe audited** — 5 current occurrences; each gets a `// SAFETY:`
   justification or is removed.
6. **Tests at three tiers:** unit (exists, ~260), **integration** (missing —
   full `handle_key → apply_command → undo/redo` loops, multi-cursor
   document-state assertions, per-target smoke tests), and benchmark
   regression gates (`cargo bench` baselines for keystroke latency and
   frame assembly).
7. **Every public item documented** (already warned; becomes enforced).
8. **No deferrals without a conversation.** Any item that looks like it
   should move phases stops the work and goes to Tom. Agreed moves are
   *relocations in this document*, not deletions.

---

## 3. Target architecture

```
crates/
  iridium-core/       # THE KERNEL. Document (ropey), commands, undo tree,
                      # cursors/selections, input handling (incl. auto-pairs,
                      # auto-indent, pair-skip — repatriated from TS),
                      # search/replace, fold state, viewport MATH,
                      # config, themes. NO wgpu. NO web. Compiles anywhere.
  iridium-syntax/     # Native tree-sitter: highlights, injections, folds,
                      # locals, textobjects. Used by TUI/desktop directly;
                      # compiled to WASM grammar bundles for web.
  iridium-render/     # GPU frame builder: the compositor (hoisted from
                      # wasm.rs) + glyphon/quad/gutter/minimap primitives.
                      # Takes a wgpu surface; knows nothing of canvases
                      # or windows. Shared verbatim by desktop and web.
  iridium-tui/        # Terminal face: cell grid, diff renderer, crossterm,
                      # kitty keyboard protocol, synchronized output,
                      # OSC 52 clipboard, unicode-width cell math.
  iridium-desktop/    # winit shell over iridium-render.
  iridium-web/        # wasm-bindgen event translator over iridium-render.
                      # (today's iridium-bindings, gutted to a translator)
  iridium-lsp/        # LSP client on the kernel. Transport-pluggable:
                      # stdio (terminal/desktop), WebSocket (web/Meridian).
apps/
  iridium/            # The terminal binary — first standalone host.
packages/
  @iridium-editor/core           # single TS package (duplicate tree deleted)
  @iridium-editor/syntax-worker  # worker (re-export shims resolved)
```

The dependency rule that makes the trifecta real: **nothing in
`iridium-core` may depend on a rendering or platform crate.** CI enforces
this with a `cargo check -p iridium-core --no-default-features` gate.

---

## 4. Phases

Ordering is by dependency, not by importance. Nothing here is optional;
"phase N" means "after phase N−1", never "maybe".

### Phase 0 — Baseline (immediate)
- Land or explicitly shelve the uncommitted WIP on
  `feature/lsp-hover-completion` (three tangled threads: LSP seams, Bun
  migration, package rename — untangle into separate commits).
- Resolve the package rename (`@iridium/*` → `@iridium-editor/*`)
  everywhere or revert it (decision D4).
- CI skeleton: check, clippy (report-only initially), test, per-crate
  no-default-features check. Ratchet to `-D warnings` per crate as phases
  complete it.
- Record `cargo bench` + keystroke-latency baselines so improvement is
  measurable, not vibes.

### Phase 1 — Kernel honesty
Everything is in Rust, benefits all three faces, and fixes the "unfinished
nerves" that made the editor feel 80% right.

Correctness:
- Multi-cursor made real: Backspace/Delete/cut/copy/word-ops act on all
  cursors; navigation preserves secondary cursors; same-line insert
  position adjustment actually written (`insert_text`'s comment currently
  describes code that doesn't exist); merge-on-overlap after every op.
- Undo: cursor position restored on undo/redo of plain edits; compound
  grouping flattened (currently quadratic clone-and-nest per grouped
  keystroke); the two divergent paste implementations unified.
- Regex replace: `$1`/`$name` capture-group expansion. [DONE, wave 1 —
  with verify-or-skip semantics for stale matches after Norn review]
- Case-insensitive literal search: byte-length desync on Unicode fixed.
  [DONE, wave 1]
- `SpanIndex::count` overreport fixed. [DONE, wave 1]
- Search-state invalidation on document mutation (root cause behind the
  stale-match hazards found in wave 1's Norn review: `Editor::apply_command`
  never invalidates or refreshes `SearchState`, so any edit leaves recorded
  match ranges pointing at the wrong text — the literal-mode replace path
  still trusts them). Invalidate or re-run the active search on edit.
- UTF-16-as-bytes eliminated: all byte-offset math moves to the Rust side;
  TS asks the editor for offsets instead of computing them from JS strings.

Behavior repatriated from the TS controller into the kernel:
- Auto-pairs (insert, skip-over, pair-delete), auto-indent on Enter,
  bracket-block Enter expansion, code-fence expansion — driven by
  `EditorConfig` + tree-sitter indent queries, not a hardcoded TS `"    "`.
- Tab honors `insert_spaces`/`tab_width`; Shift+Tab outdents (line and
  selection); indent/outdent selection blocks.
- Bracket matching (highlight matching pair; jump-to-match motion).

New kernel verbs (the "text transformations" soul — small now, cheap now):
- Line ops: move line/selection up/down, duplicate, join, delete line.
- Comment toggling (line + block) via language config.
- Column/rectangular selection model (Alt+drag comes in each face's phase).
- Add-cursor-above/below; select-all-occurrences; skip-occurrence (Ctrl+K
  Ctrl+D equivalent); undo-last-cursor.

Tests: the integration tier lands here, including multi-cursor
document-state assertions that would have caught every bug above.

### Phase 2 — Structure (the great unbundling)
- Crate split per §3. The kernel compiles GPU-free; `render` stops being a
  hard dependency of editing logic (`Viewport` math moves kernel-side).
- Compositor hoisted from `wasm.rs` into `iridium-render` as a
  platform-neutral frame builder; `iridium-web` becomes an event translator.
- Performance cliffs removed while the code is open:
  - Per-frame O(document) `doc_to_visual` rebuild → incremental/cached
    (fold state already has the O(log n) machinery; use it).
  - Whole-document fold rescan per edit → incremental via tree edits.
  - Full-document `getContent()` traffic per keystroke across the WASM
    boundary (currently 3–4 copies) → cursor-context queries.
  - Idle frame scheduling so cursor blink actually blinks; caret gets the
    ~50ms eased glide (feel, not decoration — decision D6 if contested).
- Duplicate TS tree (`crates/iridium-bindings/ts/`) deleted; single
  `@iridium-editor/core` package; worker shims resolved.
- Web IME [proposed relocation 2026-07-13, awaiting Tom]: hidden-editable
  element + composition-event plumbing driving the core's existing (and
  currently unfed) `input/ime.rs` state machine, plus preedit rendering.
  Discovered in wave 3b review: no composition path has ever existed on
  web — the canvas is not an editable element, so CJK/dead-key IME never
  engages. Wave 3b ships the `insertText()` seam this layer will use.
- 500-line cap lands on everything touched (which is most of the worst
  offenders: `wasm.rs`, `keyboard.rs`, `core.rs`, `pipeline.rs`).

### Phase 3 — Syntax depth
The tree-sitter organ gets its blood supply. All of this exists as vendored
queries or dead code today; it gets wired, not written from scratch.

- Injections: markdown code fences, JS/CSS in HTML, template literals,
  front-matter. Layered parse trees with per-layer highlighting.
- Query predicate evaluation on the native path (`#match?`/`#eq?` are
  currently ignored — native highlighting mis-fires on constants/builtins).
- Capture precedence: priority-aware overlap resolution replacing greedy
  first-wins; `#set! priority` honored.
- Incremental highlight invalidation in the *shipping* path (the smart
  changed-ranges splice currently lives only in an unused class); worker
  gets request coalescing + viewport priority; full-content-per-keystroke
  posting replaced with edits.
- Fold detection moves from hardcoded node-type lists to the vendored
  fold queries; `#region` markers implemented (currently a stub).
- Locals/scope-aware highlighting (parameters vs variables) — the last of
  the vendored query set.
- Indent queries drive auto-indent (replacing Phase 1's regex heuristic —
  the heuristic ships first so the feel improves immediately; this upgrades
  it. Both are in the plan; neither is deferred).

### Phase 4 — Terminal
The native highlighter finally gets its job; the kernel gets its first
non-web face.

- `iridium-tui`: front/back cell buffers, diff-only repaint, synchronized
  output (BSU/ESU) for atomic frames, truecolor with 256-color degradation,
  kitty keyboard protocol with legacy fallback, bracketed paste, OSC 52
  clipboard, `unicode-width` cell math (CJK/emoji double-width — the fixed
  char-width assumption dies here permanently, including in mouse hit
  testing).
- Search & replace UI — the engine has existed for months with no UI
  anywhere; the terminal gets the first one (find bar, match count,
  regex/case/word toggles, replace/replace-all).
- `apps/iridium`: open/save (atomic write, external-change detection),
  statusline, go-to-line, theme loading (VS Code theme import already
  works), folds, the full multi-cursor verb set.
- Feel gate: keystroke-to-paint < 5ms measured; 100k-line file torture
  test; no flicker under fast scroll; startup < 50ms.
- Decision D1 (modal vs non-modal keymaps) lands before this phase starts.

### Phase 5 — Desktop
- `iridium-desktop`: winit window + surface, the same `iridium-render`
  frame builder web uses, native clipboard/IME (kernel's IME state machine
  is already sound — it needs real events fed to it), file association
  ("double-click a file, it opens").
- Scrollbar (currently nonexistent in any face) and minimap wired here
  per decision D2.

### Phase 6 — LSP
- `iridium-lsp` on the kernel: JSON-RPC, lifecycle, incremental
  didChange with *correct* position encoding (the Phase 1 offset work is
  the prerequisite that makes this safe).
- Transports: stdio (terminal + desktop), WebSocket (web/Meridian).
- UI in each face using the already-built seams (`positionToPixel`, hover
  callbacks, diagnostic gutter colors from the current branch): diagnostics
  (squiggles + gutter + hover), hover, completion menu, signature help,
  go-to-definition/references, rename, format-on-demand.
- Meridian keeps its server-side span path as an alternative intelligence
  source — same seams, different transport (decision D5 governs defaults).

### Phase 7 — Ship
- npm publishability: wasm build script (none exists today — `pkg/` is
  committed by hand), compiled JS + `.d.ts` (no raw-TS shipping), the
  monorepo-relative WASM path replaced with a real loader; examples updated
  to consume the published shape.
- napi target: per decision D3 (delete, or resurrect against the kernel).
- Global CI ratchet: `-D warnings` workspace-wide, allow-count 0,
  unwrap-count 0, module-cap check, doc coverage, bench regression gates.
- Docs: embedder guide per face, architecture doc, theme format doc.

---

## 5. Cross-phase workstream: performance truth

Not a phase — a standing rule. Every phase must leave these numbers
recorded (CI artifact, not vibes): keystroke-to-visible latency per face,
frame assembly time at 1k/10k/100k lines, memory at 100k lines, startup
time per face. The Zed-memory-blowup failure mode is prevented by
*measuring*, not by intending.

---

## 6. Ghost-ship dispositions

Every built-but-unwired feature gets an explicit fate. Nothing stays dead.

| Item | Disposition |
|---|---|
| Minimap (complete, never called) | Wire in desktop + web (Phase 5); explicitly absent from TUI. Needs D2 confirmation. |
| Native Rust highlighter | Becomes the TUI/desktop highlighter (Phase 4). Predicate evaluation fixed in Phase 3. |
| Smart incremental highlight splice (unused class) | Promoted into the shipping worker path (Phase 3); unused class deleted. |
| Vendored injections/indents/outline/textobjects/folds queries | Wired in Phase 3 (injections, indents, folds, locals) and Phase 1/4 (textobjects → structural selection verbs). Outline query: exposed via API for hosts (Phase 6 era), not a core UI. |
| `FrameTimer` (never consulted) | Compositor adopts it in Phase 2 (frame budget + adaptive work). |
| Cursor blink (can't blink) | Idle frame scheduling in Phase 2. |
| `tab_width`/`insert_spaces`/`auto_indent` config | Wired in Phase 1. |
| Dead `ThemeUniforms`/`SyntaxUniforms` GPU structs | Deleted in Phase 2 (per-glyph attr coloring is the chosen model). |
| `UndoNodeInfo` / node metadata | Wired in Phase 1 (undo-tree navigation API — the tree is the feature; hosts get to see it). |
| Web-component `readonly` attribute (declared, never applied) | Wired in Phase 2 TS cleanup. |
| napi target (stale) | Decision D3. |
| `syntax_stubs.rs` brace-matching fallback | Deleted in Phase 2 — after the crate split, no build exists that needs a fake syntax layer. |

---

## 7. Decision register (need Tom)

- **D1 — Keymap model.** Modal (vim-grammar), non-modal (VS Code-style),
  or a keymap layer supporting both? Shapes Phase 1 input restructuring;
  must land before Phase 4. *Recommendation: keymap layer, non-modal
  default, modal as a keymap — the kernel's command architecture already
  fits this.*
- **D2 — Minimap.** Wire (desktop/web) or delete entirely? *Recommendation:
  wire — it's finished, tested, and Rams would keep a part that's already
  perfect.*
- **D3 — napi target.** Delete, or resurrect as a headless kernel binding
  (useful for Norn/server-side text manipulation)? *Recommendation: delete
  now, resurrect from the kernel later if a real consumer appears — a
  stale parallel API is worse than none.*
- **D4 — Package naming.** Commit to `@iridium-editor/*`? (Half-done today.)
- **D5 — Default intelligence source on web.** Local worker or
  Meridian-served spans by default? (Worker is currently disabled by
  default with a comment pointing at Meridian.)
- **D6 — Caret glide.** Eased cursor movement animation: on by default,
  off by default, or absent? (Pure feel question; blink is separate and
  landing regardless.)
- **D7 — Cypher/SQL.** The original spec required them; native crates were
  incompatible in Jan. The web path can carry any grammar via
  tree-sitter-builder WASM bundles today, and native can vendor grammar C
  sources. Back in scope (which phase?), or out?
- **D8 — Harness.** Tom's dispatch/Norn harness, Claude-native workflows,
  or mixed per-phase? (Execution mechanics, not scope.)

---

## 8. Working agreement

- Comms via Meridian DM; plan changes land in this file via PR-style diffs
  Tom can see.
- No silent deferrals (§2.8). No "minor issues" category exists.
- Each phase ends with: hygiene at standard for touched modules, tests
  green at all tiers, perf numbers recorded, a runnable demonstration in
  at least one face, and a Meridian summary of what moved.
- Phases are dependency-ordered but internally parallelizable across
  agents; the phase boundary is a quality gate, not a pace limit.
