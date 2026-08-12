# #109 — the capture ratchet, and what stood behind it

13 Aug 2026. Picked up because both live threads (#113's grammar, #108's click)
are blocked on Tom and this one is kernel-only, fully gateable, and needs
nobody's hands.

## The defect, restated after measuring it

`every_vendored_capture_maps_to_a_highlight_type` read capture names off
**compiled** queries. `query::compiled` returns `None` for a language with no
grammar linked, the loop `continue`d, and so the ratchet's coverage was a
function of *which of the fourteen grammars happen to be linked* rather than of
what the tree vendors — while reading, to anyone who found it, as a check over
the vendored queries.

Eight vendored query directories ship a `highlights.scm` and have no compiled
query to read it from. Four are languages with no grammar (`diff`, `gitcommit`,
`gomod`, `gowork`); four are not `Language` values at all (`jsdoc`, `jsonc`,
`markdown-inline`, `regex`) and so cannot even be named through `query::source`,
which takes a `Language`.

## ⭐ The task's own measurement was wrong, in the way it warned about

The task described ten unmapped names, including `diff.minus` and `diff.plus`
**used by `diff`**. They are not. `diff/highlights.scm` writes both names only
inside two `;; TODO:` comments proposing them as a future refinement of the
`@string` / `@keyword` it captures today. The measurement had been taken with a
naive `@name` scan, which read comment prose as captures — the exact trap the old
test's own comment cited as its reason for reading compiled queries instead.

The true set is **six**, and it took the corrected scan to say so:

| name | written by | was |
| --- | --- | --- |
| `diff.plus` `diff.minus` `diff.delta` `diff.delta.moved` | `gitcommit` | unmapped |
| `label.regex` `operator.regex` | `regex` | unmapped |

Two more were found that the ratchet **structurally cannot see**, because they
map — to the wrong thing:

| name | reached | should have | why it mattered |
| --- | --- | --- | --- |
| `keyword.operator.regex` | `Keyword`, via the `keyword` prefix arm | `Operator`, as its own bare spelling `keyword.operator` does | one concept, two spellings, two colours |
| `variable.other.member` | `Variable`, via the `variable` prefix arm | `Property`, as `variable.member` does | a git trailer key wore the variable colour |

> **An unmapped capture renders as plain text, which somebody eventually
> notices. A mismapped one renders as a confident wrong colour, which nobody
> does.** The ratchet only ever asked the first question.

## The build

### 1. `iridium_lang::query::capture_names` — a scan over `.scm` text

New. A lexer, not a parser: it skips `;` comments and `"` strings (honouring
`\` escapes) and reads `@name` in code position. It lives in `iridium-lang`
because that crate owns the vendored text and needs no tree-sitter to answer.

### 2. `iridium_lang::query::sources(kind)` — keyed by directory, not by language

New. `source()` takes a `Language` and therefore cannot reach the four
directories that are deliberately not languages. `sources()` iterates the
generated table by directory id, which is the only way to cover the tree.

### 3. ⭐ The oracle — the scan is checked against tree-sitter itself

`the_capture_scan_agrees_with_tree_sitter_wherever_a_grammar_can_answer`:
for every language with a grammar, the scan's set of names must equal
`Query::capture_names()`. Fourteen real query files, including `css`'s
`"@media"` string literals and `python/highlights.scm:40`'s
`(decorator "@" @punctuation.special)` — a string `@` and a real capture on one
line.

The count is asserted too, against `has_grammar`: a loop whose body never runs
passes every assertion inside it, and "no grammar happened to be linked" is the
precise shape of the defect this whole file is about.

### 4. The mapping

- `label.regex` → `Property` (joins `label`), `operator.regex` → `Operator`.
- `keyword.operator.regex` → `Operator`, `variable.other.member` → `Property`.
- Exact arms, **not** prefix arms, following the rule the markup block already
  states in `capture.rs`: a prefix arm gives every name a future vendor refresh
  introduces a plausible category with nobody ruling on it, which is the silent
  mapping the ratchet exists to surface. The task proposed prefix arms; this is
  a deliberate departure from it and the reason is the file's own standing rule.

## D-1 — the four `diff.*` names get four categories

**Ruled in this seat.** `DiffAdded`, `DiffRemoved`, `DiffModified`, `DiffMoved`.

- **Four, not three.** `diff.delta` is modified and `diff.delta.moved` is
  renamed. Folding them together reads as tidier and would mean no theme could
  ever part a renamed file from an edited one — the "one value standing for two
  sentences" defect this codebase has now paid for three times (laws 3, 6, 8).
  They share a *colour*, which is what a many-to-one colour map is for.

- **Colour: borrowed, and it is not a colour ruling.** ⚠️ No `SyntaxColors`
  field means "added" or "removed". Added takes `string` and removed takes
  `keyword` **because the vendored `diff` query names exactly those two as its
  own fallbacks in its own text** — borrowing the upstream author's stated
  choice is a weaker claim than inventing one, and weaker is right here.
  Modified and moved take `attribute`, the slot for a token that annotates the
  entity beside it, which is what "modified:" does to the path after it.

- **⛔ Not reaching `EditorColors`.** The task flagged `change_added` /
  `change_modified` as the colours that exist. They belong to the **change
  gutter** — uncommitted edits measured against the file on disk — a different
  feature answering a different question. Widening `highlight_to_color`'s
  signature would let every syntax category address the editor palette and
  would change the TUI palette's call site too, to serve four names nothing
  draws yet. Diff's own theme fields belong with #87, exactly as markup's do.

- **Nothing renders today**, and that is *why* it was ruled now rather than
  deferred: no grammar is linked for `diff` or `gitcommit`, so the decision is
  free of visual consequence and stops being free the moment somebody links
  one. Leaving it unmapped until then is how an unstyled-token defect arrives
  on the same day as a new grammar and gets blamed on the grammar.

- **Revert cost:** four enum variants, four `from_capture_name` arms, three
  `highlight_to_color` arms, two tests. Nothing outside the kernel refers to
  them; there is no TypeScript mirror of `HighlightType` (checked).

## The six mutations, all measured

| # | mutation | result |
| --- | --- | --- |
| 1 | the ratchet, before the mapping | ⭐ fails naming exactly six, **and `diff` is not among the reporters** — the comment-skipping, proven on the file that caused the wrong measurement |
| 2 | scan stops skipping strings | oracle fails on `css`, naming the six invented captures `charset import keyframes media namespace supports` |
| 3 | scan stops skipping comments | ⛔ **both the oracle and the ratchet stay GREEN** — see below |
| 4 | `sources()` restricted to `Language` values | census fails: *"jsdoc ships a highlights.scm this ratchet did not read"* |
| 5 | the two mismapped names put back | ⭐ every other test including the ratchet stays green; **only** `a_suffixed_spelling_carries_the_same_category_as_the_bare_one` fails |
| 6 | `diff.delta.moved` folded onto `DiffModified` | `the_four_diff_status_names_are_four_categories` fails |
| 6b | `DiffAdded` borrows `number` instead of `string` | `the_diff_slots_borrow_the_colours_they_were_ruled_to_borrow` fails |

### ⛔ Mutation 3 is the one worth carrying

I had written in the module doc that the oracle checks the scan "including all
three cases above". **That was false and the mutation found it.** Deleting the
comment arm leaves both the oracle and the ratchet green:

- the oracle cannot see it because the only `highlights.scm` that writes an `@`
  in a comment *and* has a grammar is `awl`, whose comment mentions `@string` —
  a name that file already captures, so the set is unchanged;
- the ratchet cannot see it because the names a comment-blind scan invents
  (`diff.plus`, `diff.minus`) now **map**, so nothing is reported unmapped. The
  fix removed the ratchet's own ability to catch the regression.

The comment rule's only guards are the two unit tests in `captures.rs`, both of
which do fail, and the first of which uses `diff/highlights.scm`'s real text.
The corrected boundary is now written in that module's doc.

> **📌 The law: a fix can remove the evidence its own gate was reading.** Once
> `diff.plus` mapped, a scan that hallucinated it stopped being reportable. The
> guard for a lexer rule has to sit at the lexer, not at the consumer that
> happened to be sensitive to it before the fix.

## What is proven, and what is not

**Proven by `cargo test`:** the scan agrees with tree-sitter on all fourteen
grammared files; every capture name in all twenty-two vendored directories maps
to a category; the eight grammarless directories are named and reached; the four
diff categories are distinct and borrow the slots ruled for them; the two
mismapped names now agree with their bare spellings.

**Not proven, and not provable here:** that `DiffAdded` *looks* right. No
grammar is linked for `diff` or `gitcommit`, so no pixel in either face has ever
carried one of these categories. The first person to link one of those grammars
should look at a commit buffer before believing the colours.
