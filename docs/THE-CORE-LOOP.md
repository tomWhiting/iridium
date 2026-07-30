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

The single most important sentence for prioritisation:

> *"90 million times out of ten I'm not writing code. What I'm doing is pulling stuff out
> of text files, markdown files."*

He is a **structured-text extractor**, not a code author. Roughly **half** of his editor
time is spent in **JSON and JSONL**, mining data out of it. Markdown conversion is much of
the rest.

This does not change the identity or the architecture. It changes **which features are
core and which are garnish**, and the current plan has some of them filed as garnish.

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

### 3.1 Feature 1 is the daily driver

*"Select the largest syntax node — I use that almost every single day."* The concrete loop,
in his words: cursor lands somewhere in the middle of a JSON string field, boundaries
unknown. Press a key: the whole field is selected. Press again: the enclosing object.
Again: the array. Again: the document.

This is expand-selection / shrink-selection over the tree-sitter tree, and it is the
feature that makes the editor worth opening.

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

## 6. Open questions

- **Soft wrap.** He did not mention it, and it is the one genuinely structural feature
  that would be expensive to retrofit — it breaks the one-document-line-equals-one-screen-row
  assumption currently baked into `Viewport` and all the cell math. JSONL in particular is
  single enormous lines. Needs an answer before the terminal renderer is finalised.
- **Mouse mode in the terminal.** He referenced a tool that "does it really well" but the
  name did not survive voice transcription. Worth identifying before deciding how far
  mouse support goes.
- **File browsing.** He wants something oil.nvim-shaped — directories edited *as a text
  buffer* — rather than a sidebar tree widget. Very much in keeping with the identity, and
  notable because it is a file manager made of the editor's own primitives, which means it
  costs little once the buffer and the registry exist.
- **Which transformations**, concretely, beyond title/camel/snake case.
