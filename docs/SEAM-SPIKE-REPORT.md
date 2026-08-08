# Iridium editor-seam spike report

> ⚠️ **Dated snapshot — 12 July 2026.** Salvaged on 8 Aug 2026 from the
> `seam-spike` branch, which was then **484 commits behind `main`** and was
> deleted in the same cleanup (its tip is kept as the tag
> `archive/seam-spike`).
>
> **The code this report describes was deliberately not merged.** The spike
> carried ~1,400 insertions including 400 lines of churn in `wasm.rs`, a
> `web_delta.rs`, and underline decorations in the theme. `wasm.rs` has since
> been split, feature-gated and clippy-swept under `-D warnings`, so merging
> the branch would have been a several-hundred-line conflict against code that
> no longer exists in that shape. Anything wanted from it should be rebuilt
> against today's `wasm.rs`, using this document as the specification and the
> tag as the reference implementation.
>
> So read the **Status** column as *"was true of the spike branch on 12 July"*,
> not as a description of `main`. Every row marked **Built in spike** is
> therefore **absent from `main` today** — `applyTextDelta`, selection mapping,
> scroll preservation and underline ranges among them.
>
> What survives unchanged in value, and is why this was kept: the
> primitive-by-primitive gap table, the seven pieces of feedback on the seam
> design, and the collaboration-contract section arguing that edit *origin*
> must live on the undo-tree node rather than being bolted on in JavaScript.
> That argument still applies to `UndoTree` as it stands.

## Result

The spike proves the two highest-value missing render/edit primitives can live on Iridium's existing web surface without changing the napi surface:

- `applyTextDelta([{start, end, text}, ...])` accepts UTF-8 byte ranges against the pre-edit document, validates the complete batch before mutation, applies replacements from the end of the document, maps the primary selection, preserves pixel scroll, and records the replacements plus selection transition as one `Command::Compound` undo-tree entry.
- host-pushed underlines use `{start, end, colorClass}` plus a host-defined class-to-hex theme. They render as foreground quads and split across logical lines and wrapped visual rows.

This is a seam proof, not a declaration that the entire aion seam already exists.

## Demo and fixture provenance

Open `web-test/seam-spike/index.html` through an HTTP server after running `./web-test/build.sh`. It loads `awl_exam.awl`, copied unchanged from aion's `docs/design/aion-authoring/awl/exam/workbench/awl_exam.awl`. The 138 spans in `awl_exam.highlights.json` are an **offline fixture**, generated with tree-sitter CLI 0.26.10 by running `queries/highlights.scm` from public `tomWhiting/tree-sitter-awl` commit `fa1afb22c756175131d252380fb4ba9e4b68a321`, then converting query row/column captures to UTF-8 byte offsets.

The page establishes a visible selection on `reported`, sets a nonzero scroll offset, and offers a scripted two-range edit (header insertion plus workflow rename). One Undo reverses both changes and restores the old selection. It also pushes two differently classed underline ranges.

## Primitive-by-primitive gap report

| Seam primitive | Status | Evidence / remaining size |
| --- | --- | --- |
| Content get/set | **Exists** | Wasm `getContent`/`setContent`; controller `getContent`/`setContent`. `setContent` is a reset-style operation, not an undoable delta. |
| Change events | **Exists, adapter caveat** | Controller has `onChange`; normal input and this spike's `applyTextDelta` notify it. The low-level wasm object is pull/render oriented rather than an event emitter. A hard seam adapter should be the single mutation owner so direct low-level calls cannot bypass notifications. Small adapter task. |
| Apply text delta over byte ranges | **Built in spike** | Controller and wasm `applyTextDelta`. Ranges must be sorted/non-overlapping after normalization, valid UTF-8 boundaries, and refer to pre-edit content. Empty batches are no-ops; invalid batches mutate nothing; read-only rejects. |
| Preserve/map selection | **Built in spike for primary selection** | Anchor/head before a replacement stay fixed; after it shift by byte delta; endpoints inside or exactly at an edited range associate with the end of inserted text. Undo restores the original primary selection because `SetSelection` is inside the compound command. Secondary selections are currently carried through unchanged because the web editor does not expose multi-cursor editing; mapping those is a medium task if aion requires them. |
| Preserve scroll | **Built in spike** | `scroll_y` is not touched by delta application. Note that content inserted above the viewport does not anchor the same document line visually; the contract requested preservation of scroll, but aion should decide whether it actually wants pixel-offset preservation or viewport-content anchoring. The latter is a medium layout task. |
| One gesture = one undo unit | **Built in spike** | One `Command::Compound` is pushed. Undo/redo includes the mapped selection transition. |
| Pixel `positionToPixel` | **Remaining gap (medium)** | Iridium has internal wrap-aware cursor/layout calculations and public `pixelToPosition`, but no public inverse. Extracting the cached visual-row calculation is straightforward; semantics before first render, folded positions, and off-viewport positions need an explicit contract. |
| Layout metrics | **Partial (small/medium)** | Wasm exposes line height, line count, scroll/max scroll and maintains viewport/cursor metrics internally. There is no cohesive controller snapshot for viewport rectangle, content extent, gutter width, or character metrics. |
| Scroll events | **Remaining gap (small)** | Wheel input and programmatic scrolling exist, but the controller has no host scroll callback. Add one notification path covering wheel, cursor reveal, resize clamping, and programmatic changes. |
| Push highlight spans | **Exists** | Controller `setHighlightSpans` accepts UTF-8 byte range plus capture `type`; wasm indexes viewport spans. External callers must repush/map spans after content changes. The demo does that explicitly. |
| Primitive completion edit | **Exists at low level / controller audit needed (small)** | Existing completion application is the relevant primitive, but the hard seam should normalize it with `applyTextDelta` so all host edits share notification, selection, and undo semantics. |
| Line background | **Exists** | `setLineBackgrounds`, zero-based logical lines. |
| Gutter marks | **Exists in fixed form** | `setGutterChanges` supports added/modified/deleted color kinds. Truly generic icon/shape marks remain a medium render/API task. |
| Gutter text | **Exists** | `setCustomGutterText`. |
| Underline ranges | **Built in spike** | Generic host-named color classes and UTF-8 byte ranges; wrap-aware foreground rendering. This spike draws a straight 2 px underline only. Styles such as wave/dash, hover ownership and z-order conflict policy remain small/medium follow-ups. |
| Remote cursor markers | **Remaining gap (medium)** | Cursor quad infrastructure exists, but there is no pushed marker model, label rendering, selection tint, or viewport culling API. Do not overload underline spans for this. |
| `onMouseHover` | **Remaining gap (medium)** | Mouse position mapping exists for clicks. Hover needs pointer-move throttling, enter/leave behavior, byte/position payload semantics, and folded/wrapped hit testing exposed through the controller. |

## Collaboration contract: design only

Do not add an `origin` string only at the JavaScript layer. The undo tree must own origin metadata at each history node. A future edit envelope should carry `{origin, undoScope, replacements}` into core history, where `origin` identifies local user, remote peer, formatter, completion, etc., and `undoScope` identifies the local history stream.

Local undo should walk backward from the current branch to the nearest undoable node in the caller's scope, apply that node's inverse transformed through intervening foreign-origin edits, and create a new branch/head without deleting remote nodes. Redo is therefore branch- and scope-relative, not a global stack pop. Compound delta commands remain one node and keep origin on the compound, not independently on children. Selection state should be local-view metadata; remote edits map it but must not restore a remote peer's selection during local undo.

Iridium's current `UndoTree` already provides branching command history, but nodes contain only commands and timestamps/group data, with no origin/scope or operational transform/rebase layer. Adding fields is small; correct scoped traversal plus transforming inverses through intervening edits is **large** and belongs with the collaboration model, not this spike.

## Feedback to the aion seam design

1. Specify offsets as **UTF-8 bytes** everywhere and require character boundaries. JavaScript string indices are UTF-16 and cannot be passed directly; adapters need `TextEncoder`/parser byte offsets.
2. Define endpoint association. This spike maps either endpoint inside a replaced range, including an insertion point, to the end of inserted text. Without an affinity field this is the only deterministic policy, but suggestion insertions may need before/after affinity.
3. Define whether “scroll preserved” means identical pixel offset (implemented here) or preservation of the same top-of-viewport document anchor when edits occur above it. Those differ visibly.
4. Decorations and highlights become stale after edits. The seam should either require the host to repush them after every accepted change (current demo), or define automatic range mapping and invalidation versions. Versioned decoration snapshots are safer than silently rendering old byte offsets.
5. Return an edit receipt/version from delta application in production. The spike returns only success/error; aion will need document version, normalized delta, and resulting selection to reject stale async parser/agent output.
6. Make mutation/event ownership explicit. A controller callback is sufficient only when every mutation goes through that controller; exposing the low-level wasm object creates event bypasses.
7. Clarify remote cursors as decorations versus collaboration state. Their lifetime, labels, affinity, and viewport behavior are substantially richer than a generic colored range.

## Known spike limits

- No origin tags or local-scoped undo were implemented.
- No browser automation asserts pixels; the scratch page is the visual proof surface.
- Underlines currently use monospaced character-width geometry, matching the existing editor layout assumptions.
- Validation is atomic, but core `Command::Compound::apply` itself has no rollback mechanism if a future command fails after validation. This implementation constructs only already-validated replacements and a valid selection command. A general transactional command API would be a medium core change.
