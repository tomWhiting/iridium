# Parser Tax Map — making a keystroke's syntax cost O(edit), not O(document)

Produced 4 Aug 2026 by a full read of the keystroke path and the vendored
tree-sitter 0.26.3 sources, for the follow-up named at the end of
`docs/DESKTOP-SHELL-PLAN.md` step 3 (the 4 Aug live table,
`docs/DESKTOP-SHELL-PLAN.md:169-190`: keydown→present p50 **31.83ms** on a
10k-line .rs file vs **1.26ms** for the same bytes as .txt — so ~30ms of
every keystroke with a grammar set is tree-sitter-adjacent work). The plan
posed the question this map answers: "whether that path is unwired on the
desktop face or the parse is non-incremental is the first question of that
work" (`docs/DESKTOP-SHELL-PLAN.md:186-188`).

Citation legend: `C:` = `crates/iridium-editor/src/editor/core.rs` (2737
lines), `S:` = `crates/iridium-editor/src/editor/ast/state.rs`
(SyntaxState), `D:` = `crates/iridium-editor/src/editor/ast/delta.rs`, `T:`
= `crates/iridium-syntax/src/tree.rs`, `HL:` =
`crates/iridium-syntax/src/highlight.rs`, `E:` =
`crates/iridium-editor/src/document/edit_span.rs`, `I:` =
`crates/iridium-editor/src/span_index/interval_tree.rs`, `H:` =
`apps/iridium-desktop/src/highlight.rs`, `A:` =
`apps/iridium-desktop/src/app.rs`, `K:` =
`apps/iridium-desktop/src/keys.rs`, `TF:` =
`crates/iridium-tui/src/frame/highlight.rs`, `W:` =
`crates/iridium-bindings/src/wasm.rs`, `WK:` =
`packages/@iridium/syntax-worker/src/worker.ts`, `CT:` =
`packages/@iridium/core/src/controller/index.ts`, `B:` =
`crates/iridium-editor/benches/syntax.rs`, `P:` =
`docs/DESKTOP-SHELL-PLAN.md`, `ts:` = vendored
`~/.cargo/registry/src/…/tree-sitter-0.26.3/binding_rust/lib.rs` (the
version the lockfile pins — root `Cargo.toml:50`, `Cargo.lock:2927-2928`).
Prior art: `docs/design/RETAINED-SHAPING-MAP.md` (the change whose live
re-measure exposed this tax).

## The two load-bearing facts, up front

1. **The incremental design RUNS. The parse is not the tax.** Every content
   command computes a pre-edit `EditSpan` (C:986-988), `note_edit` applies a
   real `InputEdit` to the retained tree without parsing (S:201-206 →
   T:112-116 → `ts_tree_edit`, ts:1427-1436), and `sync` reparses **with the
   edited old tree** (S:239-241 → T:124-128 →
   `parser.parse(source, old.as_ref())`, ts:845-856). That is case (a) of
   the question — not (b) full-parse-from-scratch, not (c)
   reparse-without-the-edit. It is not merely read as correct: the in-tree
   bench asserts `full_parses` stays at 1 across every measured iteration on
   the same 10k-line fixture (B:369-378), and the whole
   edit-plus-incremental-reparse round trip is the bench's sub-millisecond
   claim (B:1-15). ~1ms cannot be 30ms.

2. **The 30ms is the whole-document span re-derive that runs after every
   parse generation.** The desktop `HighlightCache::refresh` — called at the
   top of every redraw, inside the keydown→present window (A:1084-1087) —
   sees the parse counter move and rebuilds its entire `SpanIndex` from
   scratch: `SpanIndex::new(highlighter.spans_in(tree,
   &state.document.text()))` (H:169-171). `spans_in` runs a fresh
   `QueryCursor` over the **entire tree** with no range restriction
   (HL:358-395); tree-sitter ships the restriction API for exactly this —
   `QueryCursor::set_byte_range`, "Set the range in which the query will be
   executed" (ts:3156-3169) — and nothing on the native path calls it. The
   estate already proves the scoped form works: the web face's worker runs
   range-restricted captures for any file over 200 lines (CT:1128-1130,
   WK:245). The fix is to stop re-deriving the document and derive the
   viewport.

## 1. What one keystroke actually does (desktop, language set)

The full path, with every tree-sitter call named and classified:

| # | Step | Where | tree-sitter call? | Cost class |
|---|---|---|---|---|
| 1 | winit `KeyboardInput` received; latency clock starts at receipt | A:1273-1300 | — | trivial |
| 2 | `keys::translate` → kernel `KeyEvent` | K:30-42, A:1291-1292 | — | trivial |
| 3 | `press` → `search_or_document_key` → `Editor::handle_key` | A:381, A:411-430, C:518-533 | — | trivial |
| 4 | dispatch → `consume_key_result` → `apply_command_internal` | C:712-718, C:977 | — | — |
| 5 | `compute_edit_span` against the **pre-edit** document | C:986-988, E:97-189 | — | O(log doc) rope lookups |
| 6 | `command.apply` mutates the document | C:991 | — | O(edit) rope ops |
| 7 | `note_edit`: two `byte_point` lookups, build `InputEdit`, shift the tree | C:999-1001, S:182-212, S:325-349 | **`Tree::edit`** (T:112-116, ts:1433-1436) — position shift, no parse | O(tree depth); bench: sub-10µs, parse counters provably unmoved (B:293-330) |
| 8 | `refresh_syntax` → `SyntaxState::sync`: revision agrees + dirty → **incremental** branch | C:1004-1008, C:266-284, S:235-242 | **`Parser::parse(source, Some(edited old tree))`** (T:124-128, ts:845-856) — the doc's stated contract: "you must edit `old_tree` to match the new text using `Tree::edit`" (ts:849-851), which step 7 did. Plus **`Tree::changed_ranges`** on the old-tree clone (S:236-242, T:168-176, ts:1445-1464) | reparse ∝ damage, not document; whole step bench-bounded ~sub-ms incl. an AST walk (B:332-383). `document.text()` materialises the ~320KB rope once (S:240, `buffer.rs:143-145`) |
| 9 | fold refresh consumes the delta — `SyntaxDelta::Incremental{edit, changed}` scopes the region walk | C:273-284, D:37-44, `fold_state/detect.rs:111` | reads the tree, no parse, no query | O(changed), by design |
| 10 | `press` requests a redraw | A:395-401 | — | — |
| 11 | `RedrawRequested` → `redraw` → **`HighlightCache::refresh`** | A:1324, A:1084-1087 | — | see next row |
| 12 | generation `{parses, revision}` moved (both move on every keystroke: H:144-147, S:241, `buffer.rs:286`) → **whole-document rebuild**: `document.text()` again + `spans_in(tree, &text)` + sort + dedup + `SpanIndex::new` | H:144-179 | **`QueryCursor::new` + `matches` over the entire tree, range unrestricted** (HL:358-395, ts:2944-2948, ts:2985-2990) | **O(document). This is the tax.** |
| 13 | `compose`: `resolve` queries the index for the viewport only, builds runs; retained shaping re-diffs per line and reshapes only recoloured lines | A:1106-1122, H:246-301, I:125-136 | — | O(viewport); the whole edit frame benches 7.54ms worst-case (P:148) |
| 14 | present; latency clock stops | A:1146-1153 | — | — |

Undo/redo takes the same shape: `perform_history_request` reports the span
through `note_edit` (`editor/history_nav.rs:45-56`) and calls
`refresh_syntax` (C:1113), then the redraw pays step 12 identically.

**Verdict on the plan's question:** the path is neither unwired nor
non-incremental. `compute_edit_span` lives at
`crates/iridium-editor/src/document/edit_span.rs:97`, is called on every
command the desktop face can produce (C:986 — all input reaches
`apply_command_internal`, C:962-977), covers every content command shape
including compounds (E:129-165, merged conservatively E:80-88), and returns
`Ok(None)` only for pure selection changes (E:90-91). Safety nets exist and
are not being hit on this path: an unresolvable span or missed report
degrades to a full parse via the revision comparison (S:14-18, S:225-234),
observable through `full_parses` (S:109-117). The intent — "typing never
parses [wholly]" — is what runs. What the intent never covered is that
**deriving the spans from the parsed tree was left O(document) per
generation**, and a keystroke moves the generation every time.

## 2. Where the ~30ms plausibly goes

The arithmetic frame: 31.83ms (.rs) − 1.26ms (.txt) ≈ **30.6ms** of
with-grammar work per keystroke (P:173-176). The .txt run carries the whole
rest of the pipeline — kernel edit, retained-shaping recolour, compose,
submit, present — so the difference is attributable to steps 7-9 and 12
plus the language-dependent part of step 13. Ranked:

| Rank | Cost | Where | Estimated magnitude | Reasoning |
|---|---|---|---|---|
| **1** | **`spans_in`: unrestricted query walk over the whole tree** | H:170, HL:358-395 | **~25-30ms — the dominant term** | The only O(document) tree-sitter work on the path. The cursor visits every node of a ~320KB / 10k-line Rust tree against the vendored Zed query (219 lines, 58 capture-bearing lines, `crates/iridium-syntax/src/languages/queries/rust/highlights.scm`), materialises a `HighlightSpan` per capture — plausibly 10⁴-10⁵ spans at Rust token density — then `sort()` + `dedup()` over the lot (HL:389-392). Everything else on the path is measured or bounded below 1-2ms, so by elimination this term carries what remains |
| 2 | `SpanIndex::new` — Lapper interval-tree build | H:169-171, I:59-77 | low single-digit ms | Documented O(n log n) (I:20, I:43); a second full sort of the same 10⁴-10⁵ intervals plus a Vec rebuild. Real, but a sort of already-nearly-sorted fixed-size records is cheap next to a query walk with per-node pattern matching |
| 3 | Two whole-document `String` materialisations | S:240 and H:170 → `buffer.rs:143-145` | sub-ms each | ~320KB rope-to-String copies; memcpy-class. Wasteful (twice per keystroke for the same revision), not the headline |
| 4 | Incremental reparse + `changed_ranges` + fold delta walk | S:235-242, C:273-284 | ~sub-ms, **bench-pinned** | B:342-383 times exactly this (note_edit + sync-with-incremental-reparse + a tree walk) on the same 10k-line fixture and asserts via counters that no iteration degraded to a full parse; the plan's verification bar for it is 1ms (B:10-15) |
| 5 | Per-keystroke allocation | HL:359-363, H:149-159 | noise | `QueryCursor::new` per call (one FFI alloc, ts:2944-2948) and the spans Vec. **Not** on the list: the `Query` itself is compiled once per process into a `'static` cache (HL:292-295, `crates/iridium-syntax/src/query/mod.rs:69-70`) and the compiled `Highlighter` survives every rebuild for the same language (H:149-159). No parser or query object is constructed per keystroke anywhere on the path |
| 6 | Language-dependent compose work | H:246-301, P:148 | bounded ≤ ~7.5ms, mostly shared with .txt | With a language, every typing frame is a retained-shaping miss (the highlight generation moves, H:172-173), so the viewport re-resolves and recoloured lines reshape — but the .txt live run also pays a per-keystroke miss (document revision moves), so only the recolour *delta* lands in the 30.6ms, and the whole edit frame benches 7.54ms with the cache path included |

**What a discriminating measurement needs (not run here):** three timers
around the interior of `HighlightCache::refresh` — `document.text()`,
`spans_in`, `SpanIndex::new` — logged under the existing `IRIDIUM_LATENCY`
apparatus, or equivalently a criterion bench of `spans_in` alone on the
existing 10k-line fixture (B:109-120 already generates it). Falsifiers,
stated so the diagnosis is testable: if `spans_in` on that fixture comes in
under ~5ms, rank 1 is wrong and the remainder must be hunted in ranks 2 and
6 (a span count far above 10⁵ would move rank 2 up; a pathological
recolour-reshape would move rank 6 up). If it comes in at ~25-30ms, the
diagnosis is confirmed and stage 1 below is the whole fix.

Alternatives considered and set aside as the primary suspect, with reasons:
the reparse degrading to full (ruled out — counters asserted in B:369-378
and tested in `ast/state` tests; nothing on the desktop path skips
`note_edit`); fold detection (delta-scoped by construction, C:273-284,
`fold_state/detect.rs:111`, and D:1-7 records that making it O(edit) was
this exact fight fought once already); search refresh (skipped entirely
with no active query, C:310-312); `DocumentHighlighter` (the older
parse+highlight wrapper in `crates/iridium-editor/src/syntax.rs:84-234` is
re-exported at `lib.rs:85` but **no face calls it** — it is dormant on this
path, and a cleanup candidate, R6).

## 3. The fix, staged

Target invariant: **a keystroke's syntax cost is O(edit) + O(viewport),
never O(document)**. The parse side already honours it; the span side is
the work.

### Stage 1 — viewport-scoped span derivation (the fix that moves the 30ms)

**Kernel, iridium-syntax:** add `Highlighter::spans_in_range(&self, tree:
&Tree, source: &str, range: Range<usize>) -> Vec<HighlightSpan>` — the body
of `spans_in` (HL:358-395) with one added line,
`cursor.set_byte_range(range)` (ts:3156-3169), before the match loop.
Captures whose nodes straddle the range boundary are still yielded by
tree-sitter with their full extents (the cursor restricts where patterns
*execute*, and the web worker's overlap-filter at WK:248-252 exists because
of exactly this), which is what a multi-line string crossing the viewport
edge needs — the resolver already clamps spans to visible content
(H:281-296). `spans_in` stays for callers that legitimately want the whole
document; its doc gains the cost warning this map exists to record.

**Face, the cache:** `HighlightCache::refresh` grows a covered-window
input: the frame's viewport byte range widened by an overscan margin
(recommend ±100 lines; the web face ships 50, CT:1167-1168). Rebuild
triggers become: parse generation moved (as today, H:144-153) **or** the
requested window escapes the covered window. The rebuild derives
`spans_in_range` over the covered window only — O(viewport), tens of
microseconds-class — and `SpanIndex::new` indexes hundreds of spans instead
of tens of thousands. `redraw` already has everything the window needs
before `refresh` runs (`scroll_y` A:1091, surface height A:1103,
line-height via the compositor's config; the byte range is two rope lookups
the kernel exposes).

**Contracts preserved, explicitly:**

- `generation()` — "moves whenever a subsequent `resolve` could answer
  differently" (H:56-63, the retained-shaping R1 ruling) — keeps its exact
  meaning: a window-escape rebuild produces a new answer for content
  outside the old cover, so it bumps the generation like any rebuild.
  Sub-window scrolling (within overscan) rebuilds nothing and bumps
  nothing; the compositor's own `ShapeKey` viewport rows handle re-resolve
  on real scrolls (RETAINED-SHAPING-MAP §2 row 2).
- `language_active()` — the bridge/void split from the no-language fallback
  fix (H:64-73, H:130-143) is untouched: it is about whether a language is
  set, not about what the index covers.
- `rebuilds` stays the testable observable (H:53-56, H:182-190); tests gain
  the window dimension (§5).

### Stage 2 — edit-scoped skip (only if stage 1's measurement says so)

After stage 1 the per-keystroke derive is O(viewport). If measurement shows
that still matters, two refinements exist, both with in-estate precedent:
(2a) when `SyntaxDelta::Incremental.changed` (S:242, D:37-44) does not
intersect the covered window, skip the re-derive and *shift* the covered
spans by the edit — the pure span-shift math already exists and is proven
on the web face (`crates/iridium-bindings/src/highlight_span_shift.rs`, applied
at W:1103-1130); (2b) re-derive only `changed ∩ window` and merge. Both
need the delta at the face, and today `take_delta` has exactly one consumer
by design — fold detection, taken destructively (S:149-158, C:273). That
seam must not be widened casually; see R3. Stage 2 is deliberately **not**
recommended for the first landing: it is the fiddly half, and stage 1 alone
should take the tax from ~30ms to sub-ms.

### Alternative priced and declined — async derive, web-style

Move the whole derive off the input path (worker-style: derive whole-doc or
range spans asynchronously, apply late, shift meanwhile — CT:1117-1150,
W:1088-1130). Declined for the native faces: it buys latency the
viewport-scoped synchronous derive already delivers, at the price of a
staleness protocol (the web face needed the span-shift machinery to stop
*visible* miscolouring during the async gap, W:1088-1101 records the
flicker bug that forced it). Synchronous-and-correct at O(viewport) is
strictly simpler. Named because it is the honest second road, and because
its existence proves the range-scoped query works in production today.

## 4. The other faces: who shares the tax, where the fix lands

| Face | Keystroke syntax cost today, per the code | Affected? |
|---|---|---|
| **Desktop** | Steps 7-14 above: O(edit) kernel + **O(document) span re-derive** per keystroke, synchronous, inside keydown→present | The finding. Fixed by stage 1 |
| **TUI** (`apps/iridium` → `iridium-tui`, kernel `syntax` feature on, `apps/iridium/Cargo.toml:26,30`) | Identical pattern, near-identical code: `Highlighting::refreshed` rebuilds whole-document `SpanIndex::new(highlighter.spans_in(tree, &state.document.text()))` on every parse generation (TF:59-92, the rebuild at TF:82-84), called from `Frame::render` on every draw (`frame/mod.rs:160`, `frame/mod.rs:336-338`); the desktop cache is explicitly its port (H:9-13). **The TUI pays the same tax on every keystroke with a language set** — unmeasured only because the latency apparatus is desktop-side | Yes — same fix, same shape. See R2 for sharing the implementation |
| **Web** | The wasm kernel builds **without** the `syntax` feature (`crates/iridium-bindings/Cargo.toml:55-61` — `web` does not pull `syntax`); `SyntaxState` runs on stubs whose `parse`/`edit`/`reparse` are no-ops (`syntax_stubs.rs:207-231`), so the main-thread keystroke pays **zero** tree-sitter. Parsing happens in a JS worker: incremental `tree.edit` + `parse(content, oldTree)` (WK:150-160, WK:211-219), and for files over 200 lines the capture pass is **already viewport-scoped** — `query.captures(root, startPoint, endPoint)` over viewport+50-line buffer (CT:1128-1130, CT:1154-1174, WK:186-262). Results land asynchronously via `setTreeSitterHighlights` (W:1666-1717, index rebuild W:1707-1712, generation bump W:1713-1715); the synchronous frames in the gap stay honest via span-shift (W:1088-1130). Per-frame resolution is viewport-scoped in the resolver (W:2796-2860) | No — the web face is already the shape the fix aims at. Untouched |

**Blast radius, honestly:** `spans_in_range` lands in **iridium-syntax**
(kernel — both native faces inherit the capability). The cache change lands
in **each native face** (H and TF) — or once in the kernel if R2 rules to
hoist the duplicated cache. Nothing in `SyntaxState`, the edit path, the
compositor seam, or the bindings crate changes. The web face is untouched.

## 5. Proof plan

**Already red, recorded:** the live discrimination, P:169-190 — 31.83ms
.rs vs 1.26ms .txt, same bytes, same session protocol.

**What exists in-tree, and what it can and cannot see:**

- `crates/iridium-editor/benches/syntax.rs` — `note_edit` (sub-10µs claim,
  parse counters asserted unmoved, B:293-330) and
  `expand_after_edit_10k_lines` (edit + incremental reparse + AST walk,
  counter-asserted incremental every iteration, B:342-383). These pin the
  kernel path and **must stay green through the fix untouched** — they are
  the proof that stage 1 has no business in `SyntaxState`.
- `crates/iridium-editor/benches/compose_frame.rs` benches with **no
  language by design** (compose_frame.rs:53-58) — it can never see this
  tax; that is a feature (renderer-only meaning), keep it so.
- `crates/iridium-editor/benches/editing.rs` — document ops only.
- `crates/iridium-syntax` has **no benches directory**. Nothing anywhere
  measures `spans_in` — the one O(document) call on the keystroke path is
  the one call with no ruler on it. That absence is how this shipped.
- `HighlightCache` generation-gate tests (H:355-548) — the
  `rebuilds`-observable pattern the new tests extend.

**New evidence the fix needs:**

1. **The red number, headless**: a criterion bench beside `syntax.rs`
   reusing its fixture generator (B:109-120): `spans_in` whole-document on
   the 10k-line file — this records today's tax as a number that survives
   the fix — and `spans_in_range` over a ~90-line window with the target:
   **sub-millisecond, expect tens of µs**. Also pins the §2 rank-1/rank-2
   split (`SpanIndex::new` timed separately on the same span set).
2. **Cache observables**: keep `rebuilds`; assert per §3 — typing with an
   unmoved viewport rebuilds once per keystroke *over the window only* (the
   bench in (1) is what makes "window only" a time claim; the unit test
   makes it a count claim via a spans-derived-length observable); scrolling
   within overscan rebuilds nothing; scrolling past the cover rebuilds and
   **moves the generation** (the retained-shaping staleness matrix gains
   this row); language-removal still moves the generation without a rebuild
   (the existing H:461-502 contract, unchanged).
3. **Correctness at the seam**: a boundary-straddling span (multi-line
   string across the window edge) resolves to the same runs whole-doc vs
   range-derived — the direct analogue of the web worker's overlap filter
   reason (WK:248-252), testable by comparing `resolve` output under both
   derivations for identical content.
4. **The live re-measure**, at the next bundle refresh, same
   `IRIDIUM_LATENCY` protocol, recorded beside the 4 Aug table in P: 10k
   .rs typing p50 **sub-8ms required**, expected low single digits — the
   .txt floor is 1.26ms (P:176), stage 1 adds an O(viewport) derive plus
   recoloured-line reshapes on top of that floor, and the whole edit frame
   benched 7.54ms worst-case on a noisy box (P:148). If the .rs number does
   not land near the .txt number plus small change, §2's ranking was wrong
   and the discriminating timers in §2 say where to look next.

## 6. Open rulings for the controlling seat

- **R1 — Scope of the derive.** Viewport+overscan synchronous
  (recommended: O(viewport) per keystroke, no staleness protocol, the
  resolver's clamping already handles boundary spans) vs whole-document
  async worker-style (declined in §3 with reasons) vs whole-document
  synchronous but incremental-by-shift (that is stage 2's territory, not a
  starting point). Recommend viewport+overscan, margin ±100 lines,
  tunable; the margin is a taste knob, not a correctness knob.
- **R2 — Where the cache lives.** `HighlightCache` (H) and `Highlighting`
  (TF) are the same design twice, by documented descent (H:9-13); stage 1
  edits both or hoists one shared, generation-gated, window-covered span
  cache into `iridium-editor` (feature `syntax`, beside `span_index`) that
  both native faces wrap. Recommend the hoist — the duplication already
  cost one face an unmeasured copy of this exact tax, which is the argument
  in one sentence — but as its own commit inside the track, after the
  desktop fix is measured, so the fix's diff and the refactor's diff stay
  reviewable separately.
- **R3 — The delta seam, if stage 2 ever lands.** `take_delta` is
  deliberately single-consumer and destructive (S:149-158); fold detection
  owns it (C:273). A second consumer needs either a broadcast/generation'd
  delta or the hoisted R2 cache being refreshed *inside*
  `refresh_syntax` where the delta already flows. Recommend: defer
  entirely — decide only if stage 1's measured numbers say stage 2 is
  needed, which §3 predicts they will not.
- **R4 — Redundant `document.text()` materialisations.** Two ~320KB
  Strings per keystroke for the same revision (S:240, H:170); the
  range-derive still needs source bytes for query predicates, so the call
  does not disappear — but it can shrink (the derive needs at most the
  covered window's bytes plus whatever predicates read, and tree-sitter's
  `TextProvider` seam at ts:2987 accepts chunked text, which a rope serves
  natively). Recommend: out of scope for stage 1 (sub-ms, §2 rank 3);
  record as a follow-up candidate on the same ledger, not silently
  dropped.
- **R5 — TUI scope.** Fix the TUI in the same track (free if R2 hoists;
  near-mechanical otherwise) vs desktop-first-then-TUI. Recommend same
  track — the TUI pays the same tax today and the terminal face has no
  latency apparatus to catch a regression later.
- **R6 — Dormant `DocumentHighlighter`.** The older parse+highlight
  wrapper (`crates/iridium-editor/src/syntax.rs:84-234`) owns its own
  `SyntaxTree`, duplicating the one-tree-per-document rule `SyntaxState`
  exists to enforce (S:1-12); no face calls it (re-export only,
  `lib.rs:85`). Its `edit_bytes` convenience path also carries a documented
  coordinate compromise (T:130-138). Recommend: not this change's business
  — a separate one-commit removal or quarantine at the end of the track,
  the RETAINED-SHAPING-MAP R7 precedent exactly.
- **R7 — Bench vocabulary and placement.** The new `spans_in` /
  `spans_in_range` bench belongs beside `benches/syntax.rs` in
  iridium-editor (the fixture generator lives there and reaches through
  public API only, B:16-20) rather than starting a bench infrastructure in
  iridium-syntax. `compose_frame` keeps its no-language meaning. Recommend
  as stated; the alternative (bench in iridium-syntax against a committed
  fixture) was declined because a 300KB fixture file in the tree was
  already ruled against once (B:106-108).

## RULINGS — controlling seat, 4 Aug 2026

All seven as recommended, binding on the implementation, with one
sequencing clarification:

- **R1**: viewport+overscan synchronous derive, margin ±100 lines as a
  named tunable constant.
- **R2**: the hoist is ACCEPTED but sequenced — stage 1 first edits BOTH
  existing caches (desktop `HighlightCache`, TUI `Highlighting`)
  mechanically, so both native faces stop paying the tax in the same
  change; the shared-cache hoist into `iridium-editor` lands as its own
  separate commit AFTER the stage-1 numbers are confirmed at the next
  live measurement. Fix's diff and refactor's diff stay reviewable
  apart, and no face waits on a refactor for its latency.
- **R3**: deferred entirely; revisit only if stage 1's measured numbers
  demand stage 2.
- **R4**: out of scope for stage 1; recorded on the follow-up ledger
  (TextProvider/chunked-rope derive), not silently dropped.
- **R5**: TUI in the same track — satisfied by R2's sequencing above.
- **R6**: dormant `DocumentHighlighter` removed as its own one-commit
  cleanup at the end of the track (RETAINED-SHAPING R7 precedent).
- **R7**: bench beside benches/syntax.rs in iridium-editor;
  `compose_frame` keeps its no-language meaning.
