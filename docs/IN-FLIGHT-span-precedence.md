# One byte range, two colours — found while landing #72

#72 was supposed to need only a build. It was split out of #70 precisely
because its three captures involved no colour choice: `tag.jsx` → `Tag`,
`text.jsx` and `nested` → deliberately unstyled. Two of the three were exactly
that. The third opened a defect in every language.

Everything below was **run**, not read. The command producing each figure is in
the transcript beside it.

## The correction first

`IN-FLIGHT-unstyled-captures.md` argued for mapping `tag.jsx` like this:

> ```
> :385  (jsx_opening_element (identifier) @tag.jsx (#match? ...))   unstyled
> :389  (jsx_opening_element (identifier) @type)                     styled
> ```
> `<Foo>` is coloured and `<div>` is not, in the same file, on the same line.

**`<div>` was not unstyled.** The first run of the new test returned
`Some(Type)` for it. The `@type` pattern at `:389` is unpredicated, so it
matches lowercase names too — `div` was captured by *both* patterns, and the
mapped one won. Then, with a first attempt at a precedence rule in place, the
same assertion returned `Some(Variable)`: a *third*, earlier pattern captures
it as well.

⭐ **Reading a query tells you which patterns exist, not which ones fire
together on one node.** The write-up's claim was drawn from the `.scm` text and
was wrong in a way no amount of further reading would have exposed. The
divergence needed execution.

## The real defect

If several patterns capture the same node and more than one is mapped, several
spans are emitted over **identical byte ranges**. A test across five one-line
sources found fifteen:

```
  tsx        10..11  "<"    PunctuationBracket vs Operator
  tsx        11..14  "div"  Type               vs Variable
  tsx        20..21  "x"    Attribute          vs Property
  typescript  9..10  "f"    Function           vs Variable
  typescript 11..12  "a"    VariableParameter  vs Variable
  javascript 20..23  "foo"  Function           vs Variable
  rust        3..7   "main" FunctionDefinition vs Variable
```

Nine more in the same run. This is not a TSX quirk and has nothing to do with
`tag.jsx` — it reproduced on **unmodified code**, before any change of mine.
`fn main()` in Rust is the plainest possible case.

**Nothing downstream could break the tie.** `HighlightSpan`'s `Ord` compares
`(start, end)` and ignores the highlight, so the pair compares equal. The old
`dedup()` removed only exact triples, so both survived. The desktop resolver
then sorts with `sort_unstable` and takes the first claim on each byte
(`apps/iridium-desktop/src/highlight.rs:211,220`).

⭐ **The reason it looks right is not the reason it is right.** In every one of
the fifteen the *more specific* highlight is emitted first, because `sort` is
stable and tree-sitter yields the specific capture before the general
fallback — so the correct colour is on screen today. But `sort_unstable` is
under no obligation to preserve the order of equal elements, and above roughly
twenty elements it genuinely does not. That is any ordinary line of code. The
colour of `main` was a function of how many spans happened to share the screen,
and of the standard library's sort implementation.

⭐ **Name the circumstance in which it fails.** Here it is a long line, or a
rustc upgrade — neither of which any test would have caught, because the
failure is a wrong colour, not a wrong answer.

## The fix, and the two that were rejected

Resolution now happens at emission, in `Highlighter::spans_with`: `sort` (stable,
so emission order survives), then `dedup_by` on the range alone, keeping the
**first span emitted**.

By construction this changes no colour that is currently stable — it makes
today's appearance a guarantee instead of a coincidence. It is also general: it
holds for every language and every grammar refresh, with nothing named.

Two alternatives were tried or priced and rejected:

- **Lowest pattern index wins.** Implemented first, on the reasoning that the
  tree-sitter idiom is to place the specific predicated pattern above the broad
  fallback, so the query author has already expressed the precedence. It failed
  three tests. Generic `@variable`-style patterns sit near the *top* of these
  query files, so lowest-index gave `div` → `Variable`. It also broke
  `spans_in_range_yields_straddling_spans_with_full_extents`, which is the
  serious one: it made the winning colour depend on the byte range being
  queried, so a token could change colour on scroll.
- **Tiebreak on the `HighlightType` discriminant.** Deterministic and
  meaningless — `Type` would outrank `Tag` purely because it is declared first
  in the enum, and every intrinsic JSX element would take its fallback colour.

## What landed

| item | disposition |
| --- | --- |
| `tag.jsx` | → `Tag`. `<div>` is now a tag and `<Foo>` still a type. |
| `text.jsx` | → `DELIBERATELY_UNSTYLED` — JSX prose, same argument as markdown `text`. |
| `nested` | → `DELIBERATELY_UNSTYLED` — the recursion arm of the labelled-statement pattern; it matches a whole `statement_block`, so a colour would paint everything inside the braces. |
| span precedence | first-emitted wins, resolved in `spans_with`. |

`tag.jsx` is listed in the match arm rather than reached by a `tag` prefix arm.
Every other family there has a prefix arm, but a prefix would also swallow
`@tag.attribute` — an attribute *name* in several grammars — which would take
the tag colour silently and never appear in
`every_vendored_capture_maps_to_a_highlight_type`, the ratchet that found this
gap in the first place.

## Tests

- `jsx_intrinsic_elements_are_coloured_like_components_are` — proven red before
  the fix, twice, returning a different wrong answer each time. The `Foo` half
  is a **specificity control**: it was already styled and had to stay so.
- `no_byte_range_carries_two_different_highlights` — the invariant, over five
  languages. Proven red on unmodified code with the full fifteen-row inventory.
  It cannot be checked downstream: `Ord` ignores the highlight, so the pair is
  invisible to any sort or `dedup` a consumer applies.

## The 500-line bar

The reasoning above pushed `highlight.rs` from 478 lines to 517. Trimming prose
to pass a line count would have thrown away the record of the defect, so the
file was split on the `frame_timer/` pattern instead:

| file | lines |
| --- | --- |
| `highlight/mod.rs` | 21 — declarations and re-exports only |
| `highlight/capture.rs` | 265 — `HighlightType` and `from_capture_name` |
| `highlight/highlighter.rs` | 222 — the compiled query and the walk |
| `highlight/span.rs` | 42 — `HighlightSpan` |
| `highlight/tests/mod.rs` | 26 — declarations and the one shared helper |
| `highlight/tests/capture.rs` | 263 |
| `highlight/tests/span.rs` | 458 |

`tests.rs` was **already** over the bar at 622 lines before this work; it is now
split too. The public paths are unchanged — `mod.rs` re-exports all three types.
