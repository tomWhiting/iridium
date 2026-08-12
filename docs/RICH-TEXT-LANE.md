# The rich-text lane — verified ground and the plan

Written 12 Aug 2026. Everything under "Ground" was read off this machine —
the crates in this tree and the vendored cosmic-text source — not recalled.

## Where this came from

Waffles brought a scoping question on 11 Aug: a GitHub-grade markdown renderer
in the editor, mermaid included, keeping the feel iridium has now — and, from
the manifold side, a surface whose chrome *is* editor-drawn decoration
(chevrons, sigils, spacing) over the same lines you edit. Two hats, possibly
one engine.

The scoping answer went back the same day. **Tom released both layers on
12 Aug** through Waffles' docket round: his standard is no short sell and no
holding the fancy half back, so L1 and L2 both go rather than L1-then-wait.
Design calls during the lane route to Waffles, not to Tom.

⭐ **L1 is still built before L2, and that is dependency order, not a hold.**
L2 consumes L1's style payload; there is no arrangement in which L2 lands
first. Backlog item: **#90**.

## ▶ RESUME HERE (12 Aug 2026)

**Released and in progress. No stop is in force.** Design calls go to Waffles,
not to Tom.

Landed, ten gates green at each, all unpushed:

| commit | what |
|---|---|
| `ad449ea8` | #107 — open file / open folder / set project |
| `f46b2cf7` | #107 closed in the docs, this file created |
| `2054a497` | **L1a** — `RunStyle` through the whole seam |
| `bf832da7` | the taxonomy finding below |
| `4efce7de` | **L1b** — ten markup categories, no visual change |
| `a830d8a6` | **L1c** — `Theme::emphasis`, both native faces wired |

| `b7dd6c7c` | **L1c evidence** — a weight really does draw different glyphs |

✅ **PUSHED.** `origin/main` is at `cc40ea00` as of 12 Aug 2026, verified by
`git fetch` and `git rev-list --count origin/main..HEAD` = 0. Eleven commits
had accumulated locally before that; Waffles caught the gap. ⚠️ **Say
"committed" until it is pushed.** "Landed" was doing work in reports that the
repository state did not support — the gates were green on one machine.

**~~Next step 1: the #69 serialisation experiment~~ — ✅ DONE, and it landed on
the downside branch.** See **§10** of `docs/IN-FLIGHT-69-flaky-gutter.md`. Run
12 Aug with three arms rather than two: the flake is **the blinking caret**, in
every arm including the fully serialised one. The caret quad is 2 px × one line
= 40 pixels, and its visibility is a function of `Instant::now()` read inside
`compose`; two composes straddling the 500 ms half-interval differ by exactly
those pixels. Fourteen distinct tests across three binaries, one signature, 8
failures in 87 runs.

⭐ **It invalidated more than #69, exactly as the downside branch predicted** —
just not through parallelism. §5's rubric and §7's classification are
**withdrawn**: "contiguous band = real regression" was classifying a caret.
Fixed by pinning the blink off in the harnesses, with two instruments so the
finding cannot decay into a comment. **This also means `b7dd6c7c`'s
plain-versus-plain control was doing its job** — it was one of the fourteen,
and it failed loudly rather than letting a style difference be misattributed.

**Next step 2 is NOT mine to take.** Turning "headings are bold" on in the two
shipped presets is a **look ruling** and goes on Tom's docket through Waffles.
The mechanism and the evidence are both landed; do not flip the presets on a
seat's own judgement. `the_shipped_presets_say_nothing_about_emphasis_yet` is
what holds that line, and it is updated with the ruling in the same commit that
flips them.

**Next step 3: the markdown-inline grammar and the injection queries** — what
makes `**bold**` and `*italic*` produce spans at all, since `MarkupStrong` and
`MarkupEmphasis` are dormant until then. Then L2's cumulative layout table.

---

**How L1c was arrived at, kept because the sequencing is the lesson:**

L1c deliberately left both shipped presets empty. Turning "headings are bold"
on is an **outcome** claim, not a mechanism claim: a weight only reaches the
screen if the face the shaper resolves *has* that weight. `FontSystem::new()`
loads system fonts and `span_attrs` asks for `Family::Monospace`, so on this
machine that resolves to a family with a bold face — but that is a reading of
the code, not a thing anyone has watched happen. The evidence needed is a GPU
readback: render one line plain and one at `RunWeight::BOLD` through the real
pipeline and assert the pixels differ. `apps/iridium-desktop/tests/` already
has the harness.

⚠️ **Do not flip the presets before that test exists.** A theme asking for a
weight nothing can draw is exactly the decorative API this lane keeps naming.
`the_shipped_presets_say_nothing_about_emphasis_yet` says so in its failure
message; when the proof lands, that test is updated with the ruling in the
same commit.

Then, with the presets on: the markdown-inline grammar and the injection
queries — which is what makes `**bold**` and `*italic*` produce spans at all,
since `MarkupStrong` and `MarkupEmphasis` are dormant until then — and then
L2's cumulative layout table.

⚠️ Two constraints carried forward, both measured:

- `highlight_to_style` (`crates/iridium-editor/src/syntax.rs`) is called by the
  desktop resolver **and** by the TUI palette
  (`crates/iridium-tui/src/frame/palette.rs`) precisely so the two native faces
  cannot drift. Anything that replaces it keeps that property or the point of
  it is gone.
- `markup_emphasis_borrows_a_slot_that_is_still_plain_ink` will start failing
  the moment markup gets its own colour *fields*, and **that is the signal, not
  a breakage** — it exists to say "the borrowing is over". Delete it in the same
  commit that gives markup real fields, never before.

⚠️ **#110, found while landing L1c.** `VsCodeTokenSettings::font_style` is
parsed and read by nothing, so an imported VS Code theme loses its italics
silently. Not folded in for the same reason as #109: carrying it needs a
scope → *category* map, and the importer has a scope → colour *field* map, so
importing against what exists would lean half the keywords in a theme that
said "keywords are italic".

⚠️ **#109 is not part of this lane and must not be folded into it.** The
unmapped-capture ratchet turns out to be blind to every grammarless language,
which is why four inline markup captures went unmapped and unreported. Its fix
needs a diff/git colour ruling and a source-text capture scan — one coherent
commit of its own, with the fix and the gate landing together.

⚠️ Working tree carries an untracked `.claude/skills/` that is unrelated to
any of this and must not be committed.

## Ground

### The seam everything goes through

`HighlightSource::resolve` (`render/compositor/highlight.rs:54`) returns
`Option<Vec<(&'a str, Color)>>`. Both faces implement it. That tuple is the
whole of what a face can say about how text should look, and widening it is
what L1 *is*.

Below it, four setters in `render/text.rs` build cosmic-text attributes:
`set_text` (:443), `set_rich_text` (:457), `set_rich_text_diffed` (:508),
`set_text_diffed` (:594). Every one of them constructs
`Attrs::new().family(Family::Monospace).color(…)` — family hardcoded, colour
the only variable. Six such constructions in the file.

⚠️ `set_rich_text_diffed`'s doc states the constraint that governs any change
here: it must stay **byte-for-byte faithful** to what `set_rich_text` builds,
or the first diffed frame after a full rebuild spuriously reshapes every line.
A style payload has to reach both by the same construction.

### What cosmic-text actually supports (`cosmic-text-0.15.0/src/attrs.rs:228`)

`Attrs` carries `color_opt`, `family`, `stretch`, `style` (Normal/Italic/
Oblique), `weight`, `metadata`, `cache_key_flags`, **`metrics_opt`**,
`letter_spacing_opt`, `font_features`.

- **Weight and italic are free** — `Attrs::weight` and `Attrs::style`, already
  per-span through the `AttrsList` both rich setters build.
- ⭐ **`metrics_opt: Option<CacheMetrics>` is per-span font size *and* line
  height** (`:106-109`, `Attrs::metrics` at `:305`). **The shaper can already
  lay out a heading larger than its body text.** This materially revises the
  L2 estimate given on 11 Aug: variable line height is not blocked by
  cosmic-text at all, only by iridium's own scalar assumption below.
- ⛔ **There is no underline and no strikethrough.** The only occurrence of
  either word in the whole crate is a `//TODO: underline` in cosmic-text's own
  syntect example. Decorations are not a shaping concern — they have to be
  drawn as quads, which the two quad pipelines can already do, positioned from
  `layout_runs` glyph geometry.

### The uniform grid, and what rests on it

`line_height` is `font_size * line_height_factor`, one scalar
(`render/text.rs:311`). `char_width` is one measured advance, cached
(`:325`). 40 files mention the first, 24 the second.

What divides or multiplies by them:

| site | use |
|---|---|
| `compositor/frame.rs:71-75` | viewport virtualisation, `scroll_y / line_height` |
| `compositor/placement.rs:34` | `max_scroll_y`, `total_visual * line_height` |
| `compositor/placement.rs:96,127` | **hit test**, `(x - offset) / char_width` |
| `compositor/placement.rs:163,208` | inverse hit test |
| `render/cursor.rs:265-271` | every caret and selection quad |
| `render/gutter.rs:239-` | gutter width and line-number alignment |

⚠️ The hit test is the dangerous one. glyphon shapes proportionally and would
draw proportional text *correctly*; only the caret would land wrong. A wrong
thing that looks right is the worst failure shape available here.

### Decoration machinery that already exists

- Per-line background colours — `line_backgrounds_mut: HashMap<usize, Color>`
  (`compositor/settings.rs:188`).
- Gutter change bars, gutter band, selection, caret — all quads.
- `RoundedQuadRenderer` (`render/rounded.rs`) — real SDF arcs, hairlines, soft
  shadows; the desktop face draws its entire chrome with it in a second
  `LoadOp::Load` pass.
- Arbitrary per-visual-line gutter *text* — `set_custom_gutter_lines`
  (`settings.rs:205`), correctly in the retained-shape key.
- **Folds** — the only existing mechanism that swaps a block of text for
  something else, done at extraction (`compositor/shaping.rs:175-187`,
  appends `" ... }"`).

### The manifold hat already half-exists

`file_tree/rows.rs` draws `▸`/`▾` chevrons, two-space-per-level indent and
`+`/`✗` change markers as coloured spans over an editable buffer. Its own doc
(`:59-70`) names the exact wall L1 removes: *"the overlay's `Span` carries text
and a colour and nothing else … there is no line-through attribute to set"* —
which is why a row marked for deletion wears a `✗` instead of being struck
through.

### Markdown, measured

- `tree_sitter_md::LANGUAGE` is linked (`iridium-syntax/src/grammar.rs:90`) —
  the **block** grammar only. There is no `"markdown-inline"` arm anywhere.
- So `markdown-inline/highlights.scm` — `emphasis`, `strong_emphasis`,
  `code_span`, `strikethrough`, inline links, images — **can never run**.
  `**bold**` produces no span today.
- Block captures do work: headings (`title.markup`), list markers, table
  pipes, fence delimiters, block-quote markers, reference-definition links.
- Markdown sections map onto the `@class` text object, so heading navigation
  already works (`query/textobject.rs:25`).
- ⛔ **Injections are vendored and consumed by nothing.** `QueryKind::Injections`
  exists (`iridium-lang/src/query/kind.rs:28`) and dozens of `injections.scm`
  ship, but the only references outside the enum are in tests — one test
  comment says so outright. A ` ```rust ` fence inside markdown is not
  highlighted as Rust, in any face.
- No mermaid anything. No image or texture path in `render/` at all: grepping
  it for image/texture returns two hits, both the word "image" in a float-
  conversion doc comment.

## The plan

### ⭐ The finding that reorders L1 (measured 12 Aug, after L1a landed)

✅ **Acted on in `4efce7de`.** Kept below because it is the reasoning behind
the ten markup categories, and a category set with no recorded argument is one
the next person collapses.

**Markup captures rode on code slots, so no theme could style a heading
without styling unrelated code.** From `iridium-syntax/src/highlight/capture.rs`:

| capture | maps onto | the comment's own reason |
|---|---|---|
| `title.markup` (a heading) | `HighlightType::Keyword` | *"keyword is the most prominent slot in every theme"* |
| `link_uri.markup` | `HighlightType::String` | *"a URI is a string literal in every other grammar's terms"* |
| `link_text.markup` | `HighlightType::Function` | *"`Function` is the blue slot in most themes and link-blue is what a reader expects"* |

Those were reasonable rulings when colour was the only thing a capture could
carry — a heading borrowing the keyword *colour* costs nothing. It stops being
free the moment a capture can also carry weight: "headings are bold" would
reach every `fn` and `let` in every language, because they are the same value.

⚠️ **So the order is: taxonomy first, then theme, then markdown.** L1b cannot
be "the theme gains a weight per capture" until markup has slots of its own.
`capture.rs`'s own comment already anticipated this — *"`Property` is the
alternative if it ever reads too strongly"* — it just did not anticipate why.

**The design call, taken here rather than escalated:** the *theme* owns style,
not the capture. A capture that were inherently bold could not be un-bolded by
a theme, and "what does a heading look like" is exactly what a theme is for —
it is also the only arrangement in which the two faces cannot drift, since
both read one theme. This lines up with **#87** (theme system) rather than
competing with it: #87 gains style-per-slot instead of colour-per-slot.
Revert cost if that is wrong: the taxonomy work stands either way; only the
`SyntaxColors` → `SyntaxStyles` widening would be undone. Ratified by Waffles
on 12 Aug, in the same terms — a capture that is inherently bold is an
authority the theme cannot revoke.

⭐ **What the split found on its way through.** The vendored queries write ten
markup capture names, in two conventions: Zed's suffix form (`title.markup`)
in the markdown queries, and the nvim/helix prefix form (`markup.heading`,
`markup.link.url`) in the vendored `gitcommit` query. Both had to reach one
category — two spellings landing in different categories would style a heading
in a commit message differently from a heading in a README, which no theme
author would ever think to look for.

Four of the ten — `emphasis.markup`, `emphasis.strong.markup`,
`text.literal.markup`, `strikethrough.markup` — mapped to **nothing at all**,
and nothing reported it. They live in `markdown-inline`, which has no grammar
linked, and `every_vendored_capture_maps_to_a_highlight_type` skips any
language whose query does not compile. That is **#109**, a gate defect rather
than a rendering one, and deliberately not folded into this lane.

### L1 — style on the run path that already exists

1. A `RunStyle` in `render` — colour, weight, italic. Colour stays mandatory;
   the rest default to "same as body", so an unstyled run is byte-identical to
   today's output and the diffing setters do not spuriously reshape.
2. Widen `HighlightSource::resolve` to carry it, and both faces' resolvers with
   it.
3. Thread it through all four setters in `text.rs` by one shared construction,
   so `set_rich_text` and `set_rich_text_diffed` cannot drift.
4. Add it to `ShapeKey` — or rather, confirm `highlight_generation` already
   covers it, since style arrives by the same channel as colour. **Check, do
   not assume**: a style change that moves no key shows stale text.
5. Underline and strikethrough as quads off `layout_runs`, once there is a
   caller that wants them.
6. Link the markdown-inline grammar; make something consume the injection
   queries. Both independently valuable — injections fix code fences
   everywhere, not only in markdown.

### L2 — variable line height and advance

Replace `visual_line * line_height` with a cumulative per-visual-line layout
table (offset, height), and re-derive hit testing from `layout_runs` glyph
positions instead of the `char_width` division. Every site in the table above
consumes it, both faces and the minimap included.

⚠️ **The risk is to the feel, not to the shaping.** Retained shaping keyed on
15 inputs and per-line diffing are what make a keystroke reshape one line;
every cache key and geometry readback has to be re-derived against the layout
table, and the failure mode of getting it subtly wrong is caret drift and
jitter — precisely what Tom asked to protect. That is the real content of the
L2 estimate.

### L3 — not in this lane

Images, mermaid, rendered tables. Needs L2 first (a block's height is not a
line height), a sampled-texture pipeline beside the two quad pipelines, a
mermaid rasteriser, and a decision about what the caret does inside a block.
Not released; named here so it is not mistaken for dropped.
