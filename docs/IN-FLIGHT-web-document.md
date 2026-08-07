# #43 — extracting `WebDocument`, and the two things in the way

> ## ⚠️ STATUS, 8 Aug — step 1 done, blocker 2's stated cause is WRONG
>
> - **Step 1 (`WebHighlightCache`) landed** as #76, commit `daf1d56`. The
>   sixteen fields below are now thirteen.
> - **Blocker 2's diagnosis is corrected in place below.** It cited the plan's
>   finding 3 — "the kernel's folds go stale after every edit" — which is
>   **fixed** and has been for some time. The web face's second fold state has
>   a different cause entirely, and the fix is one line in the web face rather
>   than a kernel change. See the ⚠️ CORRECTED block in that section.
>
> The rule this cost, again: a map records what was true when it was written,
> and this one inherited a claim from a *third* document without checking it.
> A citation chain is not evidence; only the code is.

Map only. **Nothing here has been compiled or run** — the box sat at load
36.28 against 10 cores for this window, and the map is what the window was
good for. Every claim below is source read at a named line.

## The shape is already decided, elsewhere

The desktop face solved this in #46 and the answer should be **mirrored, not
reinvented**. `apps/iridium-desktop/src/app/state.rs:59`:

```rust
pub(super) struct DesktopDocument {
    pub(super) scroll_y: f32,          // physical px; the kernel's is in lines
    pub(super) file: Option<TextFile>, // the kernel does no I/O
    pub(super) syntax: HighlightCache, // keyed to the compositor's seam
}
```

Three properties of that design carry straight over, and the doc comment
states all three deliberately:

1. **The kernel `Editor` is not in it.** `Workspace` owns the editors —
   `workspace.active_editor()`, `workspace.editor(document)` — and the payload
   is only the *face's* per-document extras (`state.rs:210,241,246,251`).
2. **It rides in `Workspace<T>`** rather than a map the face keeps, because the
   rule for when it goes away — when the last tab onto its document closes — is
   already implemented and tested there.
3. ⭐ **"None of which duplicate anything the kernel holds."** That is the rule
   the web face currently breaks, twice. See below.

So `WebDocument` is *not* "move nine of `WebEditor`'s sixteen fields into a
struct". It is: `Workspace<WebDocument>` owns the editors, and `WebDocument`
holds only what the browser face needs per document that the kernel does not
already have.

## `WebEditor`'s sixteen fields, classified

`crates/iridium-bindings/src/wasm.rs:180`.

| field | scope | note |
| --- | --- | --- |
| `editor` | → `Workspace` | the kernel document; not payload |
| `scroll_y` | **document** | the direct analogue of `DesktopDocument::scroll_y` |
| `ts_highlights` | **document** | worker-delivered spans |
| `use_ts_highlights` | **document** | whether this document has them |
| `span_index` | **document** | derived from `ts_highlights` |
| `highlight_generation` | **document** | staleness counter for the above |
| `fold_state` | **document**, ⚠️ duplicate | see below |
| `fold_syntax` | **document**, ⚠️ duplicate | see below |
| `pending_edit` | **document** | edit tracking for incremental reparse |
| `surface` | app | the canvas |
| `compositor` | app | the renderer |
| `needs_redraw` | app | frame-level |
| `pending_clipboard_text` | app | transient |
| `pending_host_command` | app | transient |
| `palette_mru` | app | recency is deliberately cross-document |
| `keyboard_handler` | ⚠️ **straddles** | see below |

## Blocker 1 — four fields are one concept

`ts_highlights`, `use_ts_highlights`, `span_index` and `highlight_generation`
are a single cache with a hand-maintained invariant between its parts. The
desktop face collapsed the equivalent into one `HighlightCache`.

⭐ This is #42's shape exactly — several fields whose agreement nothing
enforces — and #42's lesson applies: the invariant may well hold today, but it
holds *by circumstance*, and a reader of any one field cannot tell which is
authoritative.

**Do this first, and separately.** Collapsing four fields into a
`WebHighlightCache` is independently verifiable, needs no decision from anyone,
and shrinks the thing #43 then has to move from eight document fields to five.

### The evidence, from the 34 call sites

Counted: `ts_highlights` 12, `use_ts_highlights` 8, `span_index` 9,
`highlight_generation` 6.

⭐ **The code already has the concept. What it does not have is a type.**

Two independent pieces of evidence, neither of which is "these feel related":

**1. The consumer already groups them.** `wasm.rs:2882` declares

```rust
struct WebHighlightSource<'a> {
    span_index: &'a WebSpanIndex,
    ts_highlights: &'a [JsHighlightSpan],
    use_ts_highlights: bool,
    document: &'a Document,
    fold_state: &'a FoldState,
}
```

built fresh each frame at `:1672-1679` from borrows of exactly these fields
plus the generation. So the bundle exists at the point of use; only ownership
is scattered.

**2. The invariant is written out three times in prose.** `highlight_generation`
is bumped at `:1222`, `:1788` and `:1837`, and each site carries its own
comment explaining the same rule — *"New spans mean a new resolution answer"*,
*"Clearing changes the resolution answer as surely as new spans do — the
span-clearing half of the generation contract"*, *"the generation contract is
about the resolver's answer"*.

Three careful restatements of one rule is a rule the type system should be
carrying. ⭐ **Name the circumstance in which it fails:** a fourth path that
mutates `ts_highlights` or `span_index` and does not bump. Nothing prevents it
— the only thing holding the contract is that all three existing sites
remembered. Consumers keyed on the generation would then serve a stale colour
resolution, and highlight staleness in this face is a bug class that has
already bitten once (#34, degenerate spans).

### Therefore the shape

`WebHighlightCache` owning all four, exposing the three mutations as methods —
set-from-JS, clear, retain-shifted — each bumping the generation **internally**.
That converts "remember to increment" into something a caller cannot get wrong,
which is the same move #42 made: replace a hand-maintained agreement with a
single owner.

The four fields become one, `WebHighlightSource` borrows the cache rather than
four separate fields, and the three prose restatements collapse to one place
where the contract is enforced instead of described.

## Blocker 2 — the folds are a genuine duplicate, and it is a known defect

`fold_state` and `fold_syntax` duplicate state the kernel already owns. This is
not new: it is the plan's finding 3 — `FoldState::update_regions` is called
only from `set_content` and `set_language`, never from
`apply_command_internal`, so **the kernel's folds go stale after every edit**,
and the web face is unaffected *precisely because* it drives its own copy from
`wasm.rs`.

So the web face is currently correct **because** it duplicates. Moving that
duplicate into `WebDocument` would bless it — and would violate the one rule
`DesktopDocument`'s design states outright.

**Recommendation:** do not carry the fold fields into `WebDocument`. Fix the
kernel refresh first (the plan already schedules this as §4.1 step 4, where it
falls out of `SyntaxState` sharing one retained tree), then let the web face
read the kernel's folds like the desktop face does. If that ordering is not
acceptable, the fallback is to move them and record explicitly that
`WebDocument` knowingly holds a duplicate and why — but that is the worse
answer and should be a deliberate choice, not a side effect of this task.

### ⚠️ CORRECTED, 8 Aug — the cause above is not the cause

Everything in the two paragraphs above about *why* the folds are duplicated is
wrong. Verified by reading, at named lines:

**1. The kernel's folds do not go stale.** `Editor::apply_command_internal`
calls `self.state.refresh_syntax()` on every content change —
`editor/core.rs:1037` — and the comment sitting on that call says so outright:
*"Before this the editor's regions were correct only until the first
keystroke."* `refresh_syntax` (`core.rs:292`) is documented as **the only place
folds are recomputed**. The plan's finding 3 was fixed and the map inherited it
unchecked.

**2. So why does the web face still keep its own?** Because the kernel's copy
is **inert in the browser**, and for a reason nothing in this map guessed:

> ⭐ **`WebEditor` never tells its `Editor` what language it holds.**

`grep set_language crates/iridium-bindings/src/` returns hits in `editor.rs`
(the napi face) and in `web_folds.rs` — and **none in `wasm.rs`**.
`create_web_editor` builds a bare `Editor::new(EditorConfig::default())`
(`wasm.rs:381`) and the comment beside it states the limitation in as many
words: *"The web surface has no way to declare a language yet."*

`SyntaxState::sync` opens with `let tree = self.tree.as_mut()?;`
(`editor/ast/state.rs:227`), and `self.tree` is `Some` only after
`set_language`. With no language the `?` returns `None`, `refresh_syntax`
returns `false` at `core.rs:293-295`, and the kernel's `fold_state` is never
given regions at all.

**The web face's second fold state is therefore not a duplicate of a working
one — it is the only one that works in the browser.**

**3. What this changes about the ordering.** The precondition for "let the web
face read the kernel's folds like the desktop face does" is no longer a kernel
fix scheduled behind a plan step. It is:

> the web face must call `Editor::set_language`.

That is the same one-line wiring `apps/iridium-desktop/src/app/files.rs:85-86`
already does per opened file (`set_language` / `clear_language` on the
extension). It is **not** free, and the cost must be named rather than
assumed — the whole `Language` question is live under Tom's extensibility
direction (`docs/IN-FLIGHT-languages.md`, decisions L-0..L-8, **unruled**), and
`wasm.rs` currently hard-codes `Language::C` as a stand-in for "fold on braces"
in two places (`:389`, `:398`). Choosing what the browser passes to
`set_language` is a language-model question, not a fold question.

**Revised recommendation, unchanged in outcome and corrected in reason:** still
do not carry the fold fields into `WebDocument`. But the blocker is **L-0, not
a kernel defect** — and #43 should not be the commit that decides how the
browser names a language. The narrow, honest version stands: leave the folds on
`WebEditor`, and record here that they stay because the web face has no
language, not because the kernel is broken.

**One thing checked and found NOT to be a defect**, recorded so nobody spends
the window on it twice: I expected the kernel refresh and the web refresh to be
brace-scanning the same document twice per keystroke. They are not — the
kernel's path exits at the `None` above before it scans. There is one scan, not
two.

## Blocker 3 — `KeyboardHandler` is half app, half document

`crates/iridium-editor/src/input/keyboard/mod.rs:111-143`. It holds:

- `keymap: KeymapStack` — **app-wide** configuration
- `key_hints: KeyHintIndex` — derived from the keymap, **app-wide**
- `resolver: KeymapResolver` — the pending-sequence state machine
- `sticky_columns` + `sticky_state` — **per-focus**, per document

The resolver's own doc comment settles which side it is on: *"per-focus state
with the same lifetime as the sticky columns below, and must be abandoned by
the same host events that invalidate them."*

⭐ **Name the circumstance in which each wrong answer fails.** One handler per
document duplicates the keymap stack per tab — memory, plus a config reload
that reaches some tabs and not others. One handler shared across documents
carries sticky columns between them — switch tabs, press ↓, and the cursor
lands on the column you left in *the other file*. Neither is acceptable, so the
handler has to be split rather than assigned.

This one is not #43's to solve, and #43 must not pretend it is. The narrow
version — leave `keyboard_handler` on `WebEditor` for now, since the web face
has exactly one document today — is correct and honest, provided the limit is
written down where #44 will find it. **#44 (wasm exports for the workspace) is
where it becomes load-bearing**, because that is the commit that makes two
documents possible.

## Order

1. **`WebHighlightCache`** — collapse the four highlight fields. No decision
   needed, independently verifiable.
2. **Kernel fold refresh** — plan §4.1 step 4, so the face can stop
   duplicating. Or the recorded fallback.
3. **`WebDocument` + `Workspace<WebDocument>`** — carrying `scroll_y`,
   the cache, and `pending_edit`.
4. **Split `KeyboardHandler`** — app config from per-focus state. Belongs with
   #44, and #44 cannot ship two documents without it.

## The constraint over all of it

`wasm.rs` is gated on `all(feature = "web", target_arch = "wasm32")`, runs to
**3,160 lines**, and contains **zero** `#[cfg(test)]` blocks. The only gate
that compiles it is `cargo check … --target wasm32-unknown-unknown`, and
nothing anywhere executes it.

⭐ So every step above is verifiable only by the compiler, which is exactly why
step 1 goes first: a field collapse is a change the compiler *can* fully check,
because it enumerates every reader. Steps 2 and 3 move behaviour, and behaviour
is the thing nothing here can test. That asymmetry should drive the ordering,
not convenience.
