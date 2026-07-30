# Iridium — The Core Loop

**Written 2026-07-30**, from Tom's account of what he actually does with an editor all day
and what he is trying to replace. This document exists because that account **reorders the
roadmap**. `PLAN.md` sequenced the work by dependency; this says which features make the
thing *worth using at all*, which is a different question and needs to be visible.

---

## 1. What is being replaced, and why

**Zed.** Lineage: Sublime Text (until it started charging) → VS Code ("death by a million
plugins") → Zed. Not VS Code forks; explicitly not Cursor/Windsurf.

He came to Zed for the speed — *"it feels like the keys are appearing before I even hit
them"* — and **stayed for the text control**: text transformations, tree-sitter
integration, and the command palette.

Two things ended it:

1. **Zed 1.0 went agent-native** and removed his single favourite feature (see §5).
2. **It leaks catastrophically.** Roughly every ten days, an idle Zed with a project open
   consumes **120–150 GB** and bricks the machine. He has lost work to it.

He was seriously considering forking a pre-1.0 Zed to get the good parts back. Iridium is
that, done properly.

The "toaster" identity in `PLAN.md` §1 was independently re-derived by Tom in this
conversation from the other direction — he described wanting the Dieter Rams of text
editors without recalling the name. That is a good sign the stated identity is real and not
retrofitted: **the platonic ideal of a boring toaster that toasts bread perfectly and
survives a nuclear bomb.**

---

## 2. The reframe: this is not primarily a code editor

The single most important sentence for **prioritisation**:

> *"90 million times out of ten I'm not writing code. What I'm doing is pulling stuff out
> of text files, markdown files."*

By volume he is a **structured-text extractor**, not a code author. Roughly **half** of his
editor time is spent in **JSON and JSONL**, mining data out of it. Markdown conversion is
much of the rest.

### 2.1 But extraction is the volume, not the point

This correction came directly from Tom after an earlier draft of this document
over-indexed on extraction, and it is the more important half:

> *"I don't want to get lost in there that... I want to enjoy writing again. I used to quite
> enjoy writing and I don't really anymore. I don't want to feel like, oh, this is going to
> be longer than four sentences, so I'm going to dictate it or have an AI write it up for
> me. I want to enjoy doing that again."*

So the goal is not a data-mining tool that also edits text. **The goal is a writing surface
good enough that writing stops being a chore he routes around** — and the structural
manipulation is what makes it powerful enough to be worth living in.

The practical test for any decision: *does this make writing four-plus sentences feel
inviting rather than avoidable?* A feature that speeds up JSON extraction but makes prose
feel worse has failed. This is also the sharpest statement yet of why AI stays out of the
kernel: the point is to want to write, not to delegate writing.

Neither half changes the identity or the architecture. Together they change **which
features are core and which are garnish**, and the current plan has some of them filed as
garnish.

---

## 3. The must-have feature set, with honest current status

Ordered by how much he actually uses it.

| # | Feature | Status today |
|---|---|---|
| 1 | **Syntax-node selection** — expand/shrink selection by tree-sitter node | **NOT BUILT** |
| 2 | **Command palette** | **NOT BUILT** (no command registry exists) |
| 3 | **Multi-cursor / multi-select** | **BUILT** ✅ |
| 4 | **Text transformations** — title/camel/snake case etc. | **NOT BUILT** |
| 5 | **Regex search** | **BUILT** ✅ (engine; needs a UI surface) |
| 6 | **Undo tree** with real branch navigation | **ENGINE BUILT**, navigation unreachable |
| 7 | **Fuzzy file open** | **NOT BUILT** |
| 8 | **Lightweight and fast, forever** | architectural — the whole point vs Zed |

### 3.1 Feature 1 is the daily driver — and it is bigger than one command

*"Select the largest syntax node — I use that almost every single day."* The concrete loop,
in his words: cursor lands somewhere in the middle of a JSON string field, boundaries
unknown. Press a key: the whole field is selected. Press again: the enclosing object.
Again: the array. Again: the document.

**Do not scope this to that one example.** Tom was explicit on the point:

> *"I don't want that one example to be pulled out as the whole thing. It is the entire
> abstract syntax tree control thing... It's not just that I want to be able to select the
> largest node, I want to be able to navigate everything by syntax node."*

So the requirement is a **full AST navigation and selection model**, of which
expand/shrink is one verb. The verb set to design for:

- expand selection to the enclosing node; shrink back along the exact path taken
- move to next / previous **named sibling**
- ascend to **parent**, descend to **first child**
- jump to next / previous node **of a given kind**
- select a node's **inner** vs **outer** range (the textobject distinction — `i"` vs `a"`)

Every one of these is a registry command (§3.2), which is another argument for building the
registry first.

**Shrink is the trap.** It must retrace the exact path expand took, or it drifts into a
different node than the one you came from. This is structurally the same bug as `redo`
picking the wrong branch of the undo tree, which was found and fixed in this repo on
2026-07-30 (commit `f0e8a80`). The fix there was a `preferred_child` pointer recording the
active path. An expand/shrink selection stack needs the same discipline. Do not re-make
that mistake in a new place.

**What it needs:** `iridium-syntax` must expose the parse tree for positional queries. The
good news is the tree is **already retained** — `Highlighter` holds `self.tree: Option<Tree>`
(`crates/iridium-syntax/src/highlight.rs:444` and `:535`) and is already incrementally
re-parsed across edits. It is simply not public. What is missing is a node-at-position API
over it plus parent/child/sibling navigation, and a selection stack in the kernel so
shrink retraces exactly what expand did — the same `preferred_child` problem already solved
in the undo tree, and it must not be re-solved wrongly.

**textobjects.scm is vendored for 14 languages** and `outline.scm` for 13, so the query
material for structural selection and for a symbol palette already exists in the repo,
unwired.

### 3.2 The insight: features 2 and D1 are the same piece of work

The keymap layer (decision **D1**, settled 2026-07-30 — keymap layer, non-modal default,
modal as a keymap) and the command palette both require the identical foundation:

> **Every editor action becomes a named, described, addressable value in a command
> registry** — not a branch in a `match` statement.

A keymap is then *keys → command name*. A palette is *fuzzy search over the same
registry*. Build the registry once and get the keymap layer, the palette, discoverability,
rebindability, macro capability, and a clean scriptable surface for AI hosts (which must
attach via the public API only) all at once.

This is a strong argument for doing the registry **first**, before either consumer. Doing
the palette as a bolt-on later would mean either a second source of truth for actions or a
retrofit through the whole input layer.

Text transformations (feature 4) then cost almost nothing: each is one registry entry
wrapping a `Command` over the selection. They are only "not built" because there is
nowhere to put them yet.

---

## 4. Proposed reprioritisation

`PLAN.md` currently orders: Phase 2 structure → Phase 3 syntax depth → Phase 4 terminal.
That puts his #1 daily feature (node selection, a Phase 3 capability) *after* the terminal
face, and the palette nowhere at all.

Proposed, for approval — not applied to `PLAN.md` yet:

1. **Command registry** — the shared foundation for D1 and the palette. Do it first.
2. **Syntax-node expand/shrink selection** — expose the retained tree, add the selection
   stack. The feature that earns the editor its keep.
3. **Text transformations** — cheap once the registry exists.
4. **Terminal face** (in progress) — the face he lives in.
5. **Palette UI + fuzzy file open + regex-search surface** — the interaction layer.
6. **Undo-tree branch navigation** — engine is done, needs reachable API and UI.

The GPU-free kernel gate (landed, commit `0bdcf43`) is a prerequisite for 4 and is done.

---

## 5. The notebook — wanted, but explicitly not in the kernel

Zed's **original** agent panel, before 1.0 replaced it with a chat sidebar, was his
favourite feature "by far, and it wasn't even close". It was **not a chat interface**. It
was a **notebook**: write prose, hit Ctrl+Enter, a pill drops in with the model's response;
go back, edit responses, rework them. A Zed plugin added a slash-command that ran **shell
commands into the same notebook**. He describes it as a genuinely collaborative workspace,
and it is the thing he most wants back.

He is also unambiguous that this is not what the editing surface is for:

> *"That's not what the surface is for. The surface is to feel good about writing again."*

And on agent orchestration: he does not want another tool offering to spin up a thousand
agents — he already has Claude Code, Meridian and Norn for that.

**This fits the existing architecture exactly, and must be built that way.** The notebook
is a **host application** over Iridium's public API — a Frame app or a standalone host —
never a kernel feature. `PLAN.md`'s AI-separability law and Frame's ratified **ENGINE-1**
already require this. The kernel's obligation is to be a good enough text surface that a
notebook can be built on it: the command registry (§3.2) is what makes that host
scriptable, and Frame's `frame:code-editor@v1` contract is the precedent for how a host
drives the editor without reaching inside it.

Filed as: **later, wanted, and out of the kernel.** Not deferred quietly — deliberately
placed.

---

## 6. Decisions taken 2026-07-30

### 6.1 Soft wrap — YES, on by default, toggleable

> *"Text wrap, yeah, 100%. It'd be nice to be able to turn it off, but text wrap is pretty
> essential. It's just a nightmare having to scroll all the way across a page just to see
> the end of something. Definitely text wrap as probably for me the default, but
> customizable for whoever."*

**This is the structurally expensive one and it must be designed in from the start**, not
retrofitted. Soft wrap breaks the assumption that one document line equals one screen row —
an assumption currently baked into `Viewport` (`render/viewport.rs`) and every piece of the
cell math, which derives all of its geometry from `line_height`.

What it forces, concretely:

- A **visual-line vs document-line** distinction throughout the viewport layer. Scrolling,
  `visible_line_range`, `document_line_at_y`, `screen_y_for_line` and cursor motion all
  become operations over *visual* rows that map many-to-one onto document lines. Vertical
  cursor motion moves by visual row (what a writer expects), not document line.
- Wrap is a **layout** concern, so it belongs in the kernel's pure layout half, not in a
  face. All three faces must agree on where lines wrap, or the terminal and GPU faces will
  disagree about what line the cursor is on. This is a direct argument for the shared
  backend-neutral paint model in `TRIPLE-FACE.md` §4.
- It interacts with folds (already supported) — the composition of wrap and fold is the
  fiddly part, and existing fold-aware viewport methods must keep working.
- Wrap width, wrap-at-word-boundary vs character, and indent continuation are all config.
- Turning it off must restore the current horizontal-scroll behaviour exactly.

### 6.2 Terminal input — `terminput` + `terminput-termina`

The tool Tom could not name in transcription is
[`terminput-termina`](https://crates.io/crates/terminput-termina). Resolved and compiled
against locally: `terminput` 0.5.15 (2,311 lines) plus the adapter
`terminput-termina` 0.3.1 (435 lines, two conversion functions each way for events, keys and
mouse). Both compile clean on this toolchain.

Adopted, for one decisive reason beyond Tom's preference. `terminput` ships not just an
event model but a **parser** (824 lines) *and* an **encoder** (661 lines) — it can turn
events back into terminal byte sequences. That means **the entire terminal input path
becomes testable headlessly and deterministically**: synthesise key events, encode to bytes,
feed them through the real parser. No pty, no live terminal, no flaky integration harness.
Given that the analogous gap on the web face (no DOM test infrastructure) is a known,
logged weakness, buying deterministic input tests for ~400 lines of adapter is cheap.

The secondary benefit is churn insurance: termina is pre-1.0 at 0.3.x, and the adapter
absorbs its API changes.

The one honest cost: it is a second neutral event representation, since Iridium's kernel
already defines its own platform-neutral `KeyCode` + `Modifiers`. The chain becomes
`termina::Event` → `terminput::Event` → kernel `KeyCode`+`Modifiers`. That extra hop is one
mapping file and no measurable latency, and it buys the test harness. Worth it, but it
should be a deliberate choice rather than an accident — the kernel contract remains the
authority, and `terminput` must not leak past `iridium-tui`.

### 6.3 Mouse — supported, toggleable

Wanted, but must be switchable on and off. Capture has to be released on teardown along
with raw mode and the alternate screen.

## 7. Still open

- **File browsing**, oil.nvim-shaped: directories edited *as a text buffer* rather than a
  sidebar tree widget. Confirmed as the intent, not yet scheduled. Notable because it is a
  file manager built from the editor's own primitives, so it costs little once the buffer
  and the registry exist.
- **Which transformations** concretely, beyond title/camel/snake case. Tom had written a
  transformations crate previously; it is lost some thousands of commits back in another
  repo's history and is not worth recovering — *"not exactly rocket science"*.
- **Whether to adopt `chiron/crates/syntax`** — see §8.

## 8. An existing, better syntax implementation

Tom has a tree-sitter implementation he rates well above `iridium-syntax`, at
`/Users/tom/Developer/ablative/dev-ops/chiron/crates/syntax`: *"my tree-sitter chops and
implementation have gotten significantly better... it's used in a couple of other things and
it's been through the ringer a couple of times."* It uses Zed's tree-sitter queries. There is
an **LSP implementation in the same directory**, which is relevant because LSP is currently
Phase 6 and entirely unbuilt in Iridium.

**Evaluated 2026-07-30. Verdict: port the syntax crate's query layer, adopt the LSP crate.**
Every decisive claim below was verified directly, not taken on report.

### 8.1 `chiron/crates/syntax` — port the query layer, do not adopt

The crate does **not** contain the thing it was evaluated for. Verified:

- **Zero node-navigation code.** `grep` for `next_sibling|prev_sibling|descendant_for|named_child|.parent()|walk()` across its `src/` returns **0 call sites**. It is a *batch analysis* crate — parse a file, run a query, return flat tree-free captures. `capture.rs` states tree-freedom as an explicit design goal, which is the opposite of what navigation needs.
- **Zero incremental reparse.** 0 references to `InputEdit`/`Tree::edit`/`changed_ranges`; `parse.rs:106` unconditionally passes `None` as the old tree, and `ParsedSource` owns a `String` copy of the whole source. Adopting it would be a **regression** — `iridium-syntax` already does incremental reparse correctly (`highlight.rs:508-535`).
- **No highlighting and no folding at all.** `HighlightType`, `HighlightSpan`, span ordering and `FoldDetector` would all have to be rebuilt.
- **Hard dependency blocker.** iridium resolves `tree-sitter 0.26.3`; chiron resolves `0.25.10`. `tree-sitter` declares `links = "tree-sitter"`, so Cargo **refuses** a graph containing both. Adoption forces a version migration across 32 grammar crates including 6 git-pinned ones.
- **Licensing.** chiron is `MIT`; iridium is `MIT OR Apache-2.0`. Adopting MIT-only code weakens the Apache half.

**What is genuinely worth taking (~750 lines):** its query infrastructure. iridium vendors
~180 `.scm` files and loads exactly one kind (`highlights.scm`, 13 languages); chiron
registers **221 `(language, kind)` pairs across 19 kinds** with a compile-time `include_str!`
registry, a process-wide compile-once `Arc` cache, and a zero-copy capture path. That turns a
corpus iridium already ships into live capability. Also worth taking: its `Language` shape
(33 languages incl. Zig, TOML, SQL, HTML, JSONC) and its internal injection dialects
(`MarkdownInline`, `JsDoc`, `Regex`), which are the right foundation for markdown code-fence
highlighting. Keep iridium's `Option`-returning detection and its serde derives, which chiron
lacks and the wasm/napi bindings need.

**Node navigation must be written from scratch — nobody has written it.** ~400-500 lines in
a new `iridium-syntax/src/navigate.rs`. Design constraints established:

- Work in **byte offsets**. chiron's byte ranges are rope-compatible; its `start_column` is a
  *byte* offset within the line and is **not** iridium's `Position.column`, which is UTF-8
  code points (`document/position.rs:25`). Converting via the field instead of the rope is
  silently wrong on any line containing non-ASCII.
- `expand` must **skip ancestors whose byte range equals the current one**. That is the detail
  that makes expand-selection feel right, and the usual thing people get wrong.
- Keep an explicit expand/shrink **stack** so shrink is exact rather than re-derived — see the
  §3.1 warning.
- **For JSON, write direct node-kind logic, not queries.** `queries/json/textobjects.scm` is
  the single line `(comment)+ @comment.around` — verified byte-identical in *both* repos. The
  verbs that matter for JSONL mining (next/previous `pair`, select a pair's key or value,
  enclosing `object`/`array`, next array element) are trivial against tree-sitter-json's node
  kinds and unobtainable from the vendored query. Highest-leverage ~150 lines in the crate.
- Query-driven text objects can then layer on top for the 27 languages that have real ones.
  Note chiron never built this either: nothing consumes `QueryKind::TextObjects`.

**Rejected: extracting a shared crate.** The two projects want opposite things (tree-free
owned captures from disk vs a long-lived mutable tree over a rope), and a shared crate would
impose a cross-repo `tree-sitter` version-coordination obligation — precisely the thing
already blocking adoption. If the `.scm` corpus starts drifting between repos, extract a
**queries-only** crate: data, no Rust API, no tree-sitter dependency, therefore no version
conflict.

### 8.2 `chiron/crates/lsp` — adopt, when Phase 6 arrives

The better find of the two, and it changes the Phase 6 estimate substantially. Verified:
**16,148 source lines, 432 test functions, zero `unwrap`/`expect`/`panic` in production code,
and zero internal chiron dependencies.** Five clean layers (transport → client → server →
features → extension) behind an `LspWorkspace` façade with hover, completion,
goto-definition/implementation/type-definition, find-references, document and workspace
symbols, pull and push diagnostics, call hierarchy and call graphs, multi-server routing,
crash recovery with document re-sync, and health monitoring. That is essentially all of
Phase 6, already built and tested.

Plan around three things:

- **stdio + `tokio::process` only.** Gate it `#[cfg(not(target_arch = "wasm32"))]` — spawning
  `rust-analyzer` in a browser is meaningless anyway. The `Transport` trait
  (`transport/mod.rs:41`) is correctly shaped for a websocket impl (`async fn send/receive`
  on `&self`), so the web face is an unwritten impl rather than a redesign. It also needs
  iridium's tokio features widened from `["rt", "rt-multi-thread", "sync"]`.
- **9 `#[allow(deprecated)]` are imposed by `lsp-types 0.97`** and cannot be removed. They
  need either a documented carve-out in the standards or a wrapper module that concentrates
  them.
- **Real 500-line-cap violations:** `server/registry.rs` at 2,066 lines and `client/sync.rs`
  at 1,370 need splitting. Mechanical, and 432 tests cover the work.

Three Meridian-specific strings to rename (`lib.rs:1`, `server/builtin.rs:54`,
`client/lifecycle.rs:47`).

### 8.3 Separately: the `.scm` files need a NOTICE file

The vendored query corpus is Zed's, and Zed's editor tree is GPL-3.0. **Neither repo carries
a LICENSE, NOTICE, or attribution file for it.** This is pre-existing in iridium and adoption
creates no delta, but it wants resolving on its own merits before anything ships.
