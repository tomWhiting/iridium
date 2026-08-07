# #75 — the wasm clippy gate, and the 60 that were never reported

> ## ✅ CLOSED, 8 Aug. Count went 68 → 60 → 20 → 14 → 0, gate armed at `dc6a67f`.
>
> | commit | what | count after |
> | --- | --- | --- |
> | `c1bf4b0` | the 8 in `iridium-editor` (wgpu `!Send` on wasm32 only) | 60 |
> | `97725aa` | 12 `missing_const_for_fn` suppressed + 28 applied by `--fix` | 20 |
> | `729286c` | the six non-cast remainders | 14 |
> | `94953ef` | the 14 JS-boundary casts → `js_index::to_js_u32`/`to_js_i32` | **0** |
> | `dc6a67f` | `.github/workflows/ci.yml` — the gate, armed last | — |
>
> ```
> cargo clippy -p iridium-bindings --no-default-features --features web \
>     --target wasm32-unknown-unknown -- -D warnings
> → exit 0
> ```
>
> **The GPU-free kernel is now the only ungated configuration**, and it is
> still a burn-down rather than a flag.
>
> The inventory and reasoning below are kept as written — the line numbers are
> stale by design, and the arguments are the part worth reading.

Every figure here comes from a command named beside it, run on this tree.

## The thing #75 had wrong

#75 says *"8 errors hiding in `render/pipeline.rs` and `render/web.rs`"*.

```
cargo clippy -p iridium-bindings --no-default-features --features web \
    --target wasm32-unknown-unknown -- -D warnings
→ exit 101
  error: could not compile `iridium-editor` (lib) due to 8 previous errors
```

⭐ **`iridium-editor` is a dependency of `iridium-bindings`, so it compiles
first and the build aborts there.** The 8 are not the burn-down — they are the
prefix of it visible before the abort. Everything in `wasm.rs` was downstream
of a compilation that never happened.

Re-measured without `-D warnings`, over the metric `.github/workflows/ci.yml`
names (unique `(lint, file, line)` via `--message-format=json`):

| | unique |
| --- | --- |
| total, before `c1bf4b0` | **68** |
| `crates/iridium-editor/src/render/pipeline.rs` | 4 |
| `crates/iridium-editor/src/render/web.rs` | 4 |
| `crates/iridium-bindings/src/wasm.rs` | **60** |
| total, after `c1bf4b0` | **60** |

⭐ **The general shape, worth keeping:** a `-D` gate on a crate with
dependencies reports the *first* unit that fails, not the work. Its error count
is an ordering artefact. Any burn-down estimated from a denying run is
estimated from a prefix — measure with warnings left as warnings, or the
number means nothing.

**On the CI comment's 102.** It records 102 for this exact invocation, metric
and date stated (2026-08-05). It is now 68. That is not the comment being
wrong — it is real work since (#36, #37, #40, #42, #76 all touched `wasm.rs`),
and the drift is checkable *only because* the comment named its metric and its
date. That convention earned its keep here.

## Closed at `c1bf4b0` — the 8

All one situation: wgpu's `Adapter`/`Device`/`Queue` are `Send + Sync` natively
and neither on `wasm32`, where the web backend wraps JavaScript objects that
cannot leave their thread. Same source, opposite verdict per target.

- `clippy::future_not_send` × 4 — nothing spawns these futures onto a thread
  pool. `pollster::block_on` natively, `wasm_bindgen_futures` in the browser.
  This build has **no shared-memory threading on `wasm32` at all**, so the
  property the lint protects cannot be violated on the target where it fires.
- `clippy::arc_with_non_send_sync` × 4 — the suggestion is `Rc`, and taking it
  would fork `RenderPipeline`'s field types per target. The native path needs
  `Arc`; one struct cannot hold both.

`pipeline.rs` compiles on both targets → `cfg_attr(target_arch = "wasm32",
expect(...))`, because `#[expect]` is strict in both directions and an
unconditional one is an unfulfilled expectation on every native build.
`web.rs`'s sites are inside `#[cfg(target_arch = "wasm32")] mod wasm` → the
attribute is unconditional and cannot go unfulfilled.

## OPEN — the 60, all in `crates/iridium-bindings/src/wasm.rs`

Line numbers are as of `c1bf4b0`. Regenerate with the JSON invocation above
before working from them; **they shift on the first edit.**

### ⛔ `cargo clippy --fix` DELIVERS NOTHING HERE — do not reach for it

Tried, measured, and worth the paragraph so nobody tries it again:

```
cargo clippy --fix --allow-dirty --lib -p iridium-bindings \
    --no-default-features --features web --target wasm32-unknown-unknown \
    -- --no-deps
→ exit 0, and the working tree is UNCHANGED
  error: errors present after applying fixes to crate `iridium_bindings`
    = cause: error: can only #[wasm_bindgen] non-const functions
        --> crates/iridium-bindings/src/wasm.rs:1233:9
       1233 |     pub const fn is_read_only(&self) -> bool {
    ...and seven more of the same
```

Two separate things, both load-bearing:

**1. `missing_const_for_fn` is categorically WRONG on this file's exports.**
`#[wasm_bindgen]` rejects `const fn` outright. Clippy does not know that, and
marks the suggestion **MachineApplicable** anyway — so its own auto-fix emits
code that cannot compile. They are **unfixable, not unfixed**, and need a
suppression with the reason rather than a fix.

> ⚠️ **Two corrections to this paragraph, both found by doing it** (`97725aa`):
>
> - It said *"at least 8 of the 12"*. It is **all 12** — the `--fix` log names
>   exactly twelve `can only #[wasm_bindgen] non-const functions` errors. The
>   "8" was me reading a truncated range and hedging on the wrong side.
> - It said they need an `expect`. **`#[expect]` does not survive this proc
>   macro**: the lint falls silent and all twelve then report
>   `unfulfilled_lint_expectations` against the attribute's own line. They are
>   `allow`, with the lost strictness stated at the site. See §2 of the
>   retrospective at the end of this file.

⭐ **The discriminator, and it is exact:** `f08c888` — one commit earlier in
this same stint — took this lint's advice on `WebHighlightCache::bump` and was
right to. `bump` is a *private plain-Rust method*. Same lint, opposite correct
answer, and what separates them is **whether the function crosses the
`wasm_bindgen` boundary**. Check that before touching any of the 12.

**2. One bad suggestion rolls back the whole file.** `cargo fix` applies the
batch, recompiles, and reverts everything if the result is broken. So the
"40 machine-applicable" figure below is not 40 free edits — it is **zero**
until the const ones are excluded, and there is no per-lint switch for `--fix`.
Hand edits, in batches, verifying as you go.

⭐ **The general rule:** *machine-applicable* is clippy's claim about its own
suggestion, not a fact about your code. A proc macro can forbid what the lint
proposes, and neither one knows about the other.

### Group A — mechanical, compiler-verified (43)

| lint | n | lines |
| --- | --- | --- |
| `map_unwrap_or` | 16 | 1454 1493 1973 1991 2015 2034 2059 2083 2132 2156 2170 2173 2203 2207 2213 2780 |
| `missing_const_for_fn` | 12 | 1235 1534 1689 1696 1795 1801 1807 2585 2608 2614 2620 2765 |
| `uninlined_format_args` | 7 | 327 337 354 369 1260 1305 1773 |
| `doc_markdown` | 4 | 165 288 295 1705 |
| `manual_let_else` | 2 | 2142 2186 |
| `clone_on_copy` | 1 | 1414 |
| `trivially_copy_pass_by_ref` | 1 | 2115 |

⚠️ **This grouping is mine and it is partly wrong — prefer the applicability
split.** Measured, machine-applicability cuts differently: `manual_let_else`
and `trivially_copy_pass_by_ref` carry **no** machine-applicable suggestion
despite being mechanical, and `missing_const_for_fn` carries one that **does
not compile** (above). The real split is:

| | n |
| --- | --- |
| suggestion exists and is sound | 28 (`map_unwrap_or` 16, `uninlined_format_args` 7, `doc_markdown` 4, `clone_on_copy` 1) |
| suggestion exists and is unsound here | 12 (`missing_const_for_fn`) |
| no suggestion | 20 |

`missing_const_for_fn` is the same lint that turned gate 5 red on `f08c888` —
and there it was right. See the discriminator above.

### Group B — needs judgement, one at a time (17)

| lint | n | lines |
| --- | --- | --- |
| `cast_possible_truncation` | 13 | 1802 1808 1814 2398 2404 2410 2416 2636 2677 2760 2766 2773 2782 |
| `cast_possible_wrap` | 1 | 2782 |
| `future_not_send` | 2 | 304 2789 |
| `too_many_arguments` | 1 | 493 |

⚠️ **The 14 casts are the whole risk in this task.** They sit on the JavaScript
number boundary, and #40 did exactly this work for line numbers and found a
**real defect** — a JS-supplied line reaching `usize` through a bare `as`. So
the prior here is *not* "these are noise". Each one needs the same question
asked: **name the input for which the cast and the value it stands for
disagree.** A blanket `#[expect]` across them would be the wrong answer, and
would be the answer that looks like progress.

The 2 `future_not_send` in `wasm.rs` are likely the same wgpu situation as the
closed 8, but that is a guess until read — do not assume it.

## Why the gate is not armed yet

Adding

```yaml
- name: Clippy on the wasm target
  run: cargo clippy -p iridium-bindings --no-default-features --features web \
         --target wasm32-unknown-unknown -- -D warnings
```

to `.github/workflows/ci.yml` turns CI red on the next push while 60 remain.
**The gate and the burn-down land together, gate last** — the same discipline
`ci.yml`'s existing comment states for the `--all-features` job: *"Adding a
stricter flag to a job that cannot fail buys a stricter report and no
enforcement."*

Note also, from that comment and still true: `-D warnings` goes **after `--`**,
never in `RUSTFLAGS`, or the `block v0.1.6` future-incompat note in the
dependency graph fails the job for code we do not own.

## The constraint over all of it

`wasm.rs` is 3,160 lines, gated `all(feature = "web", target_arch = "wasm32")`,
with **zero `#[cfg(test)]` blocks**. Nothing executes it. The only verification
available for all 60 edits is the compiler and this clippy run.

⭐ That asymmetry should pick the order: **Group A first** — the compiler fully
checks those. Group B's casts change *behaviour at a boundary nothing tests*,
and each deserves the treatment #40 gave its one cast, not a sweep.

#76 showed the crack worth widening here: `web_highlight_cache` got real tests
by carrying `any(target_arch = "wasm32", test)` instead of the target gate,
because it is plain Rust with no `#[wasm_bindgen]`. Any Group B cast whose
logic can be lifted into a plain-Rust helper becomes testable by the same move.
That is the better fix wherever it applies.

---

# How it actually went — the four things worth keeping

Written after closing, because most of these contradict what the map above
predicted before starting.

## 1. `--fix` was zero, then it was 28. The order is the whole trick.

`cargo clippy --fix` applied all 40 machine-applicable suggestions, failed to
recompile, and rolled the entire file back — because 12 of them made
`#[wasm_bindgen]` methods `const`, which that macro rejects outright. One bad
suggestion poisons the batch, and there is no per-lint switch for `--fix`.

⭐ **Suppress the unsound suggestions first, then `--fix` works.** With the 12
`missing_const_for_fn` sites carrying an `allow`, the same command applied the
remaining 28 cleanly. That took the tool from useless to doing 60% of the task.

## 2. `#[expect]` does not survive `#[wasm_bindgen]`

The codebase prefers `expect` over `allow` because it is strict in both
directions. It is **unusable across this proc macro**: with `#[expect]` in
place, `missing_const_for_fn` falls silent *and* all twelve report
`unfulfilled_lint_expectations` against the attribute's own line. Twelve
suppressed warnings become twelve new ones.

So those sites are `allow`, and the protection given up — `allow` will not tell
us if `wasm_bindgen` ever permits `const fn` — is stated at the site rather
than quietly accepted.

## 3. The suppression boundary has to match the reason's boundary

The obvious move was one attribute on the `#[wasm_bindgen] impl WebEditor`
block. **Wrong:** twenty private helpers live inside that block, and on those
the lint is right — `WebHighlightCache::bump` took its advice in `f08c888`,
correctly, one commit earlier the same night.

⭐ The discriminator is *whether the function crosses the `wasm_bindgen`
boundary*, so the suppression stops there too. Same lint, opposite correct
answer, decided by something neither the lint nor the block knows.

## 4. The casts: a vacuous lint is still worth obeying

13 of the 14 were `usize as u32`, which **cannot** truncate on `wasm32` —
`usize` is 32 bits there, and clippy's message names 64-bit targets as the
hazard. An `allow` was defensible.

It was still the worse answer. That soundness is a fact about the *target*,
held nowhere and checked by nothing, and `wasm64` exists. `u32::try_from(v)
.unwrap_or(u32::MAX)` needs no assumption at all, so there is nothing left to
hold true — and it moved to `js_index`, beside the inbound `index`, where the
host compiles it and tests reach it.

The 14th was different and the difference mattered: `get_fold_end_line` returns
`-1` for "no fold", and `usize::MAX as i32` is exactly `-1`. Its lint —
`cast_possible_wrap` — names **32-bit** targets, which is the one we build for.
Unreachable at 2^31 lines, but "unreachable" was a fact about document sizes
rather than about the function. Same shape as the stale span index in #76.

⭐ **A lint that is vacuous today is a claim about your build configuration, not
about your code.** Obeying it cheaply is worth more than proving it vacuous
expensively.

## What the map got wrong, for the record

- "8 errors" → 68. A `-D` gate reports the first *unit* that fails, and
  `iridium-editor` is a dependency; it aborted there. **An error count from a
  denying run is an ordering artefact.**
- "43 mechanical / 17 judgement" → the real seam was machine-applicability, and
  even that was wrong, because 12 of the 40 applicable ones do not compile.
- "the 14 casts need a fresh pass, not a tired one" → this one held. They were
  the only part that needed a design decision rather than a mechanism, and the
  sentinel collision would have been easy to `allow` past.
