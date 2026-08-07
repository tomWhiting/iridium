# One span inside another — the half `IN-FLIGHT-span-precedence.md` left open

**Found 8 Aug, by following the sentence that document ends on.** Everything
below was **run**, not read; the command producing each figure is in the
transcript beside it.

## The sentence that was left open

`IN-FLIGHT-span-precedence.md` fixed *identical* byte ranges carrying two
colours: `spans_with` now sorts stably and keeps the first span emitted. Its
own account of the limit is exact:

> It still does not resolve **nested** ranges of different extents.

That was recorded as a reason `@nested` must stay unstyled. It is also a live
defect on its own, in both faces, on ordinary code — and the fix for it is not
the same fix, because nesting is not a tie to be broken. **Both spans are
right.** The inner one is simply more specific, and the screen can only show
one.

## What the kernel emits

A probe over fourteen languages with one small source each found **37
containments** — an outer mapped span strictly containing an inner mapped span
with a different highlight. Five languages produced them; the rest produced
none on these fixtures.

```
  typescript 10..22 "`x${y + 1}z`"  String  SWALLOWS  12..20 "${y + 1}"  Embedded
  typescript 10..22 "`x${y + 1}z`"  String  SWALLOWS  14..15 "y"         Variable
  typescript 10..22 "`x${y + 1}z`"  String  SWALLOWS  18..19 "1"         Number
  typescript 31..46 ": Array<string>" Type  SWALLOWS  39..45 "string"    TypeBuiltin
  javascript 34..41 "/ab+c/g"       String  SWALLOWS  40..41 "g"         Keyword
  python     26..36 "f\"{x!r} y\""  String  SWALLOWS  28..33 "{x!r}"     Embedded
  css        19..22 "1px"           Number  SWALLOWS  20..22 "px"        Type
  bash       16..24 "\"a $x b\""    String  SWALLOWS  20..21 "x"         Variable
```

⚠️ **These are not exotic constructs.** A template literal with an
interpolation, an f-string, `"$x"` in a shell string, and a generic type
annotation are among the most ordinary lines in each of those languages.
`: Array<string>` is the one worth staring at: the whole annotation renders in
one colour and the type argument inside it has its own, more specific answer
thrown away.

The kernel is **not** wrong to emit them. A `template_string` really is a
string and `${y + 1}` really is embedded code; the grammar authors captured
both deliberately. Emitting both and then discarding one is the defect.

## What each face does with them

Two resolvers, written independently, with the same shape and the same hole:

| | file | sort | overlap rule |
| --- | --- | --- | --- |
| desktop | `apps/iridium-desktop/src/highlight.rs:199` | `sort_unstable()` — `(start, end)` | `if span_start < last_end { continue }` |
| web | `crates/iridium-bindings/src/wasm.rs:3105` | `sort_by_key(start)` | `if span_start < last_end { continue }` |

Both walk the spans in order and skip any span that begins inside a range
already claimed. An outer span starts **earlier**, so it sorts first, claims its
whole extent, and every span nested inside it is skipped. The interpolation
renders as string; the type argument renders as the annotation's colour.

## ⭐ The divergence, which is the part that makes this a defect and not a choice

The desktop resolver states its own principle in a comment at `highlight.rs:209`:

> `HighlightSpan`'s order: start ascending, then end ascending — so at equal
> starts **the innermost span comes first and wins** the flat run.

Innermost wins. That is the intent, written down. But it only holds when the two
spans happen to begin at the same byte. The same pair of spans gets **three
different outcomes** depending on an accident of offsets:

| shape | what happens |
| --- | --- |
| identical ranges | resolved in the kernel — first emitted wins (`spans_with`) |
| equal starts, different ends | inner sorts first and wins; ⚠️ the outer is then skipped **entirely**, so the outer's tail past the inner falls back to plain foreground |
| different starts (the common case) | outer sorts first and wins; inner is discarded |

One rule, three answers, decided by byte arithmetic rather than by anything
anybody chose. That is the same defect class as the one
`IN-FLIGHT-span-precedence.md` closed — *the reason it looks right is not the
reason it is right* — one level up.

## Why this is not a colour ruling

Stated plainly because it is the question worth asking before touching
appearance:

- **No new colour is introduced**, no theme field is added, no capture is
  remapped. Every colour involved is already on screen elsewhere in the same
  file.
- **Every affected byte moves from a less specific answer to a more specific
  one that the grammar already gave.** Nothing invents a preference.
- It is what every editor with tree-sitter highlighting does; a flat-coloured
  interpolation is the recognisable symptom of a resolver that has this hole.

It *is* a visible change, and that is recorded rather than glossed: template
literals, f-strings, shell interpolations, regex literals and generic type
arguments will look different — more differentiated, never less.

## The fix

⭐ **One resolution, in the kernel, called by both faces.** The two copies of
the flat-run loop are the same algorithm written twice, which is what let the
hole exist twice. The web copy is the one that matters most for placement:
`wasm.rs` has **zero** `#[cfg(test)]` blocks and nothing executes it, so an
algorithm living there cannot be tested at all. In the kernel it can.

Shape: a payload-generic sweep that turns a set of possibly-nested spans into a
flat, ordered, non-overlapping run list with gaps marked, where the innermost
span owns each byte. Each face keeps only its own payload→colour mapping, which
is the part that genuinely differs (an enum against a `HashMap<String, Color>`).

## Status — LANDED

- [x] Kernel probe run; 37 containments inventoried across 5 languages.
- [x] Both resolvers read and the skip confirmed at named lines.
- [x] `crates/iridium-editor/src/render/runs.rs` — `flatten_spans`, 12 tests.
- [x] Desktop resolver rewired; its red test proved red first.
- [x] **Three** web resolvers rewired — the viewport path, the legacy fallback,
      and the clone the fallback no longer needs.
- [x] Nine gates green.

### The red test, and what red looked like

`an_interpolation_inside_a_template_literal_keeps_its_own_colours`, in
`apps/iridium-desktop/src/highlight.rs`, over `const a = \`x${y}z\`;`.

```
assertion `left == right` failed: the interpolated variable was swallowed by
the string span around it
  left: None
 right: Some(Color { r: 0.6117647, g: 0.8627451, b: 0.99607843, a: 1.0 })
```

⭐ `None`, not a wrong colour. There was no run for `y` **at all** — the
literal was a single slice and the interpolation had no existence downstream of
the resolver. That is the shape of this defect: not a mis-colouring to be
spotted, an absence.

### Two things fixed on the way, both in the lines being rewritten

1. ⚠️ **A panic path in the browser.** `snap_down` — clamp an offset down to a
   character boundary before slicing — existed only in the desktop face. Both
   web resolvers sliced the raw offset. Span offsets are document bytes while
   the content is the compositor's fold-collapsed extraction, so the two drift
   by the placeholder's length, and the first drift landing inside a multi-byte
   character would have panicked in the browser. The guard is in the kernel now
   and all three resolvers use it.
2. **A per-frame clone of the whole span set**, in the legacy fallback. It was
   there because the callee sorted in place, and carried three bullet points
   arguing it was affordable. `flatten_spans` sorts its own ranges, so the
   parameter is a slice and the clone and the argument for it are both gone.

### The 500-line bar

`apps/iridium-desktop/src/highlight.rs` is 843 lines, up from 806. ⚠️ **It was
already over before this change** and this did not push it over; recorded so the
next reader does not attribute it here. The split is its own piece of work — the
seam is the windowed-cache handle against the per-frame resolver, and the tests
divide the same way.

### What is deliberately not covered by a test

`wasm.rs` has no `#[cfg(test)]` blocks and nothing executes it; its two
resolvers are verified by the compiler and by the fact that the rule they now
call is tested in the kernel. ⭐ That asymmetry is the whole argument for where
`flatten_spans` lives. Gates 1 and 5 are native and never compile `wasm.rs`
(`cfg(all(feature = "web", target_arch = "wasm32"))`, `lib.rs:88`); gates 4 and
8 are its gates, and both were re-run after the last edit to it.
