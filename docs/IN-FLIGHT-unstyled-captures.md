# #70 — the nine unstyled captures, each traced to its query site

> **STATUS.** The three that needed no ruling have **landed** (#72) — see the
> correction below and `IN-FLIGHT-span-precedence.md`, which is where running
> them led. `KNOWN_UNSTYLED_GAP` is now **six** names, all of them the colour
> choices below, all still Tom's call. The rest of this document is the
> analysis as originally written, corrected in place where execution
> contradicted it.

Analysis only. **Nothing here has been compiled or run** — the box was above
its load threshold for the whole window this was written in (see
`IN-FLIGHT-box-gate.md`), so this is source reading and the implementation is
still to come.

`KNOWN_UNSTYLED_GAP` in `crates/iridium-syntax/src/highlight/tests/capture.rs` lists
nine capture names that produce no span, so the tokens they match reach the
screen in the plain foreground. The list was deliberately left as a ratchet
rather than fixed, on the grounds that choosing a colour is a presentation
decision.

**That grounds turns out to cover only six of the nine.** Three are settled by
what the capture actually matches, with no colour to choose.

## The constraint every fix here respects

`highlight/capture.rs` sets the rule already: ride along with an existing
`HighlightType` rather than add a variant, because `SyntaxColors` is a fixed
struct of concrete colours and **a new variant means a new field in every
theme and in the TUI palette.** `namespace` and `module` ride with `Type` on
exactly this reasoning; `Lifetime` set the precedent before them.

⭐ **Every recommendation below adds zero variants and touches no theme.** The
whole of #70 is arms in `from_capture_name` plus two list moves — which also
means it does not collide with #31, the light/dark theme work.

## Three that are not presentation decisions

| capture | site | disposition |
| --- | --- | --- |
| `tag.jsx` | `tsx/highlights.scm:385-387` | → **`Tag`** |
| `text.jsx` | `tsx/highlights.scm:422` | → **`DELIBERATELY_UNSTYLED`** |
| `nested` | `typescript/highlights.scm:98` | → **`DELIBERATELY_UNSTYLED`**, with a caveat |

### `tag.jsx` → `Tag`

The capture is predicated `(#match? @tag.jsx "^[a-z][^.]*$")` — lowercase
identifiers only, which in JSX are precisely the **intrinsic HTML elements**.
`HighlightType::Tag` is documented as "HTML/XML tags". The mapping is the
variant's stated purpose.

The asymmetry this leaves today is the argument for fixing it:

```
:385  (jsx_opening_element (identifier) @tag.jsx (#match? "^[a-z][^.]*$"))   unstyled
:389  (jsx_opening_element (identifier) @type)                               styled
```

Uppercase **components** fall to the unpredicated pattern and map to `Type`,
so they render styled. `<Foo>` is coloured and `<div>` is not, in the same
file, on the same line. There is no reading of that as intentional.

> ⚠️ **CORRECTED — the sentence above is wrong, and was wrong when written.**
> `<div>` was *not* unstyled. The pattern at `:389` is unpredicated, so it
> matches lowercase names too: `div` was captured by both patterns and rendered
> as a `Type`. A third, earlier pattern captures it as `Variable` as well.
>
> The claim came from reading the `.scm`, and reading shows which patterns
> exist, not which fire together on one node. Running it exposed a defect in
> every language — several mapped spans over identical byte ranges, with no
> rule anywhere deciding which colour wins. Full account, inventory and fix in
> `IN-FLIGHT-span-precedence.md`. The recommendation itself survives: `tag.jsx`
> → `Tag` is right, and it has landed.

### `text.jsx` → deliberately unstyled

`(jsx_text) @text.jsx` is the prose between elements. `DELIBERATELY_UNSTYLED`
already carries `text` with the justification *"markdown prose, which should
render in the plain foreground"*. JSX text is the same thing in another
grammar, and the existing entry's reasoning applies unchanged.

### `nested` — resolved, and it was never a gap

Initially flagged as needing the enclosing pattern read before landing. Read,
and it settles decisively.

The pattern at `typescript/highlights.scm:70-99` hijacks `statement_block` to
highlight the pseudo-TypeScript snippets an LSP returns, which are not valid
TypeScript in totality. Inside a `labeled_statement` it captures
`label: @property.name` and then alternates on the body:

```
body: [
  (expression_statement [ ... @type.name / @property.name ... ])
  (statement_block) @nested        ;; match a nested statement block
]
```

`@nested` is the **recursion arm**: it lets the alternation succeed when the
body is a nested block, so the outer `label:` capture fires, and the inner
block's own labels are then highlighted by the same top-level pattern matching
again.

⭐ **It must never carry a colour, because it matches a whole
`statement_block`.** `highlight/highlighter.rs` emits a span over the captured
node's full byte range — `node.start_byte()..node.end_byte()`. Mapping `nested`
would therefore emit a span covering an entire brace-delimited region,
competing with every token span inside it. That is not a token span, and
mapping it would be a defect rather than a fix.

The claim this paragraph originally carried — *"there is no overlap resolution
in that function"* — was true when written and is no longer. `spans_with` now
resolves identical ranges by keeping the first span emitted. It still does not
resolve **nested** ranges of different extents, which is what matters here: a
`statement_block` span and the token spans inside it have different extents, so
nothing would collapse them and the argument above stands unchanged. See
`IN-FLIGHT-span-precedence.md`.

So `nested` belongs in `DELIBERATELY_UNSTYLED` **with that reason recorded** —
it is the same category as the `_`-prefixed predicate operands, differing only
in that its author did not use the `_` convention.

**The gap is eight, not nine.** The list overstated it by one.

## Six that are genuinely Tom's call

All six have a defensible existing variant. Recommendations, with the reason
rather than just the pick:

| capture | matches | recommend | why |
| --- | --- | --- | --- |
| `title.markup` | markdown headings, ATX and setext | `Keyword` | headings are the strongest structural signal in a document, and keyword is the most prominent slot in every theme |
| `link_uri.markup` | `(link_destination)`, and the `( )` around it | `String` | a URI is a string literal in every other grammar's terms; least surprising |
| `link_text.markup` | link and image labels, `(link_reference_definition)` | `Function` | conventionally link-blue, and `Function` is the blue slot in most themes |
| `selector.class` | CSS `(class_name)` | `Type` | names an entity; identical argument to the one that put `namespace`/`module` on `Type` |
| `selector.id` | CSS `(id_name)` | `Constant` | an id is unique and constant-like — though `Type`, matching class, is the coherent alternative |
| `selector.pseudo` | `:hover`, `::before` | `Attribute` | a pseudo-selector modifies an element the way a decorator modifies a declaration |

**The two worth an actual opinion rather than a default** are `selector.id`
(`Constant` distinguishes it from class, `Type` unifies the selector family —
I lean `Type` for coherence if Tom has no preference) and `link_text.markup`,
where `Property` is the alternative if `Function` reads too strongly.

## What lands when

1. **`tag.jsx` → `Tag`** and **`text.jsx` → unstyled** need no ruling and
   should go in as soon as the box permits a battery.
2. **`nested`** after reading the enclosing pattern.
3. **The six** on Tom's ruling. If he does not care, the table above is the
   recommendation and it costs one line each.

The two existing tests do the enforcement either way:
`every_vendored_capture_maps_to_a_highlight_type` fails if the gap grows, and
`the_unstyled_lists_name_only_captures_that_are_still_unstyled_and_still_used`
fails if either list rots — a name that gets mapped but stays listed, or a
capture a vendor refresh dropped. Both lists shrink under this change and the
second test is what proves the shrink was real.
