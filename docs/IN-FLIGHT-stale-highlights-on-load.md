# #81 — `setContent` left the previous file's colours on the new one

Found 8 Aug 2026, by checking a sentence the code wrote about itself.

## The route in

`WebEditor::record_edit` carries this:

> This is also the single choke point every content mutation funnels through —
> local keystrokes and paste (`track_and_apply`), cut, `applyRemoteEdit`, and
> undo/redo (`record_whole_document_edit`) all call it **exactly once per
> mutation** — so it doubles as **the one place** tracked highlight spans get
> carried forward across the edit.

Two uniqueness claims in one paragraph, both checkable. The enumeration is
correct as far as it goes — every *edit* named does reach `record_edit` once.
What the sentence does not cover is the mutation that is not an edit.

`grep` for direct document mutations in `wasm.rs` returns three sites. Two are
fine (`applyRemoteEdit` records; the read-only `SetSelection` changes no text).
The third is `setContent`, and it replaces the whole document without producing
an edit span at all — so it can never reach the choke point, and everything the
choke point is responsible for silently does not happen.

⭐ **A choke point only funnels what is shaped like the thing it funnels.**
`record_edit` takes an `EditSpan`. A whole-document replacement has no span, so
it does not arrive — and the invariant reads as maintained because every case
anyone thought to enumerate *is* an edit.

## What was actually wrong

`set_content` already dropped two things derived from the outgoing document,
each with a comment saying why:

```rust
self.pending_edit = PendingEdit::None;   // incremental edit state
self.fold_syntax.invalidate();           // the retained parse tree
```

It did not drop the third: `self.highlights`, the worker-delivered tree-sitter
spans. Nothing else does either — `WebHighlightCache` is keyed on **its own
generation counter, never on a document revision**, so it has no way to notice
that the document underneath it was swapped.

The live path, from `packages/@iridium/core/src/controller/index.ts:1416`:

```ts
this.editor.setContent(content);
this.updateHighlights();   // fires the worker request and returns immediately
this.editor.forceRender(); // ← paints NOW
```

`updateHighlights` is fire-and-forget (`:1297`, `highlightPromise.then(…)`), so
the render on the next line draws the **new** text sliced at the **old**
document's byte offsets. Opening a file, or switching between them, showed the
previous file's colours smeared across the new one until the worker answered.

Two aggravations:

- `updateHighlights` returns immediately when there is no syntax worker
  (`if (!this.syntaxWorker) return;`). With highlighting unavailable — the
  documented fallback — the stale spans lasted **until something else happened
  to call `clearTreeSitterHighlights`**, which on most paths is never.
- This is precisely the flicker `shift_highlight_spans` was written to close,
  in its largest possible form. That method carries spans across an edit so the
  frame between an edit and the worker's reply is never wrong. The frame
  between a *load* and the worker's reply was left wrong.

Undo and redo were never affected: `record_whole_document_edit` synthesises a
span covering the whole document, and `retain_shifted` drops every span the
edit overlaps — which is all of them.

## The fix

One line in `set_content`, beside the two invalidations that were already
there, with the reasoning written down so the next person adding derived state
sees a list of three rather than a list of two.

`set_content` is the only whole-document replacement in the face — checked, not
assumed: `self.editor` is never reassigned and `editor.set_content` has exactly
one caller. So there is no sibling site to fix, which is the question rule C
(*a fix applied to the path you are standing on is not a fix to the defect*)
says to ask first.

## ⚠️ No red test exists, and that is not a shortcut

`wasm.rs` is gated `#[cfg(all(feature = "web", target_arch = "wasm32"))]`
(`lib.rs:88`). No native test can construct a `WebEditor`, and the wasm gate is
a `check` — it compiles without running a single test. `render/runs.rs:29-31`
says the same thing about its own second caller:

> Its second caller lives in `wasm.rs`, which nothing executes and no test can
> reach — an algorithm there could only ever be verified by reading it.

The established answer in this crate is extraction: `web_span_index` and
`web_highlight_cache` are both gated
`any(target_arch = "wasm32", test)` **specifically so tests reach them**, and
`lib.rs:90-104` explains both. That is the right long-term home for "what a
document replacement invalidates" — but inventing a policy object for a
three-item list is structure I would be adding on my own initiative, and
**#43 (*Extract `WebDocument` from `WebEditor`*) already owns that territory**
and is blocked behind L-0.

So: verified by reading, gates green, and the gap recorded here rather than
papered over. **When #43 lands, this invariant is the first thing that should
get a test.**

## Gates

All nine green. **2,533 passed, 0 failed** — unchanged, because no test
reaches the changed line. Gates 4 and 8 (wasm `check` and wasm clippy) are the
only two that compile it at all.
