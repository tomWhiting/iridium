# #69 — `custom_gutter_lines_miss_and_recompose_identically`

## ⭐⭐ SOLVED — READ §10 FIRST, THEN THE HISTORY

**It is the blinking caret, and it was never the gutter.** The caret quad is
two pixels wide by one line tall — 40 pixels at the harness's font size — and
whether it is drawn is a function of `Instant::now()` read inside `compose`.
Any two composes straddling the 500 ms half-interval differ by exactly those
40 pixels, **in any test**, whatever that test believed it was comparing.
Reproduced 12 Aug 2026 in fourteen distinct tests across three binaries, every
one with the same signature. **8 failures in 87 runs before; 0 in 455 after,
across load averages from 39 to 433.** §10 carries the experiment, the
mechanism, the fix and both after-columns.

⛔ **§§1–9 below are the history of getting there, and parts of them are now
known wrong.** They are kept because how the wrong answers were held matters,
and because two of them name a defect worth remembering. But **§5's rubric and
§7's classification are withdrawn** — see §10e — and no reader should apply
them. Every section that a later one overturns says so, at its own head.

---

`crates/iridium-editor/tests/retained_shaping/gutter.rs:73`.

Four sittings. The first added `support::pixels::assert_same_frame` so the
*next* occurrence says count, position, area and magnitude — deliberately not a
fix. The second eliminated two mechanisms and named a third. The third ran the
experiment the second proposed, found it cannot discriminate, ran its inverse
instead, and — chasing the recorded signature to compare against — found the
signature had no measurement behind it. The fourth varied the execution context
first, as §9a said to, and the answer fell out in twelve minutes.

---

## ⛔⛔ THE CORRECTION THAT MATTERS MOST — THE CITED SIGNATURE IS NOT A MEASUREMENT

The previous version of this file said, as the closing argument of its
surviving hypothesis:

> That signature matches the one occurrence on record exactly: **1 pixel of
> 393216, one channel, magnitude 23.** A logic error in the retained path does
> not produce one pixel; it produces a region.

**There is no such occurrence.** Traced to its only source, `docs/SESSION-STATE.md`
in commit `a1a74240`, where it appears inside a block introduced by the words
*"On failure it now says:"* — a **worked example of the new message format**,
written by hand to show what the helper would print. Not a capture. Nothing was
running.

⭐ **It is provably not a measurement, and the proof is the population.**
`describe_difference` (`tests/support/pixels.rs`) reports `left.len() / 4` — the
pixel count of the frame it was handed. Every readback target in this suite is
built `target(&gpu, WIDTH, HEIGHT)`, and `tests/support/gpu.rs:14,17` fix those
at **512 × 384 = 196,608 pixels**. Verified by sweep: there is no
`target(&gpu, …)` call anywhere in `crates/iridium-editor/tests/` with any other
dimensions, and no second width constant exists. **No frame in this suite has
393,216 pixels.** A run of this test could not have printed that line.

For contrast, the real failure captured this sitting (§4) opens
`40 of 196608 pixels differ` — the population a genuine run reports.

⭐ **And the actual original report carries no signature at all.**
`docs/SESSION-STATE.md:1216`, in full: *"`custom_gutter_lines_miss_and_recompose_identically`
is flaky under box load. Passed in the AWL battery; do not read one green run as
a fix."* Count, position, area and magnitude were never recorded, because when
it fired the helper that reports them did not yet exist. **That is precisely why
the helper was written** — and somewhere between writing it and writing this
doc, its demonstration output was read back as its findings.

### What has to be struck

Everything downstream of the invented figure:

- ⛔ *"A logic error in the retained path does not produce one pixel; it
  produces a region"* — sound reasoning from a premise that was not observed.
  The premise is withdrawn; **the conclusion is not evidence and must not be
  cited.**
- ⛔ The atlas-state hypothesis keeps its **mechanism** argument (§3 — the two
  compositors genuinely do hold different atlas state) and loses its
  **evidential** argument entirely. It was never corroborated by an observation.
  It is a candidate, not a leading candidate.

### ⭐ THE LAW THIS EARNS

> **An illustration placed in a record is indistinguishable from a measurement
> once the surrounding prose is gone.**

A reader three weeks later meets `1 of 393216 pixels differ; … magnitude 23` in
a document about a flake and reads it as the flake. The framing sentence — *"it
now says"* — does not survive the quotation. Two usable consequences:

1. **A worked example must be marked as invented at the point of the numbers**,
   not in the sentence above them. Write `<count> of <total>`, or `EXAMPLE ONLY —
   not observed`, and the misreading is unavailable.
2. ⭐ **A cited figure carries a population, and a population is checkable.**
   This one was caught by arithmetic alone: 393,216 is exactly twice 196,608,
   and 196,608 is the only frame size this suite has. The same check applies to
   every figure this seat publishes — it is the cheapest audit available and it
   is the one that fired here.

Same family as Rule M (*a green must state its population; a population is only
meaningful if it is the set the reader thinks it is*), one turn further along:
**state the population and the reader can catch you.**

---

## 1. ✅ ELIMINATED — readback synchronisation

The first guess, and it is wrong. `support/frame.rs:117` `read_pixels`:

```
queue.submit(...)               // copy_texture_to_buffer
slice.map_async(Read, callback) // result sent through an mpsc channel
device.poll(PollType::wait_indefinitely())   // checked, dies on error
receiver.recv()                 // Ok(Ok(())) required, all three arms handled
slice.get_mapped_range()
```

and `compose` (`frame.rs:98`) also ends on `poll(wait_indefinitely())` with the
error checked. That is the textbook sequence with no gap — nothing reads the
buffer before the GPU has finished writing it. A torn or stale readback cannot
be the mechanism.

The module comment records that two of the three harnesses this replaced passed
`|_| {}` as the map callback and dropped the result. That defect is already
gone; #55 kept the one version that reports.

## 2. ✅ ELIMINATED — cross-test GPU state

`support/gpu.rs:103` `gpu(harness)` builds a **fresh `wgpu::Instance`, adapter
and device on every call**. No `static`, no `OnceLock`, no `thread_local`. Two
tests running concurrently share no wgpu object, so "another test perturbed
this one's device" is not available as an explanation.

## 3. ◐ STILL OPEN, NOW UNCORROBORATED — warm and cold hold different atlas state

The test compares:

- `after` — a compositor that has composed **twice**: once with the default
  gutter (digits `0`..`200`), then again after
  `set_custom_gutter_lines(Some(["+0".."+200"]))`.
- `cold` — a **fresh** compositor that composes once, with the custom gutter
  already set.

Those two are legitimately different objects. The warm one's glyph atlas has the
plain digit glyphs cached from the first compose; the cold one has never seen
them. Different glyph sets, different insertion orders, different texture
coordinates — **and, once a growth happens, a different atlas size.**

**The test asserts their rendered pixels are byte-identical.**

⭐ **Stated as the proxy it is:** byte-identical pixels stand in for *"the
retained path recomposed correctly"*. **Name where they diverge:** any input for
which atlas layout reaches the rasterised output.

### ⚠️ CORRECTION — how atlas layout can and cannot reach the output

A previous note from this seat said flatly that *atlas position cannot bleed a
neighbour's texel, so the mechanism is unavailable*. **That over-reached, and
the half that is true does not close the hypothesis.** Both halves, measured
against the vendored source (`glyphon-0.10.0`, resolved through `Cargo.lock`):

- ✅ **Blending in a neighbour is genuinely unavailable.** `src/cache.rs:50-52`
  sets `min_filter: FilterMode::Nearest`, `mag_filter: FilterMode::Nearest`,
  `mipmap_filter: MipmapFilterMode::Nearest`. Nearest never averages two texels,
  so "the sampler smeared an adjacent glyph in at the edge" cannot happen.
- ⛔ **Selecting the wrong texel remains entirely available**, and this is what
  the earlier note missed. `src/shader.wgsl:110` computes
  `vert_output.uv = vec2<f32>(uv) / vec2<f32>(dim)` — integer atlas coordinates
  divided by the **atlas dimensions** — interpolates that across the quad, and
  `:119`/`:122` `textureSampleLevel` it with the Nearest sampler. So the UV is a
  *float derived from where the glyph sits and how big the atlas is*. A
  different position, or a growth that changes `dim` and thereby **rescales
  every UV in the atlas at once**, moves those floats; Nearest then rounds, and
  at a glyph boundary rounding can land on the adjacent texel.

⭐ So the mechanism is **narrower than the old text claimed and wider than my
correction claimed**: not blending, but selection — and atlas *growth* is the
sharp version of it, because it perturbs every glyph simultaneously rather than
one repacked glyph.

---

## 4. 🔬 THE EXPERIMENTS — this sitting

### 4a. ⛔ The experiment this doc previously proposed cannot discriminate

The old §"The experiment that would settle it" proposed giving the cold
compositor the same warm-up, so both atlases have seen the same glyphs in the
same order — *"if the difference is atlas state, the two become identical by
construction."*

**It cannot fail, so it cannot inform.** The test already passes; making the two
compositors *more* alike cannot turn a pass into a fail, and a pass was the
outcome either way. It has the same defect as a red-proof that finds no test: an
experiment with one reachable outcome is not an experiment. **Deleted, not
merely marked done.**

### 4b. ⚗️ Its inverse can fail — and did, once in 90

Run instead: **maximise** the divergence. The warm compositor composes three
times — default digits, then a gutter full of `#N`, then `+N` — so its atlas
holds `#`, the digits and `+`, packed in that order. The cold one ever sees only
`+N`. If atlas layout reaches the output at all, this is where it shows.

| condition | runs | fails | load (1m) |
| --- | --- | --- | --- |
| maximised divergence, paired | 50 | **1** | 48 → 112 |
| the shipped test, same pairs | 50 | **0** | 48 → 112 |
| maximised divergence, repeat | 40 | **0** | 112 → 26 |
| **maximised divergence, total** | **90** | **1** | |

⚠️ **1 of 50 against 0 of 50 is not a rate difference.** Fisher's exact on that
table is p = 1.0; it is *one event*, and the repeat run drew 0 of 40. Do not
report "the divergent configuration is flakier" — the honest claim is **a
mismatch is reachable in this configuration, once observed, not reproduced.**

### 4c. ⭐ The signature, which is the part worth keeping

```
maximised atlas divergence must still land byte-identical
  40 of 196608 pixels differ; the first at column 61, row 10;
  they span columns 61..=62 and rows 10..=29;
  the largest single channel difference is 149
```

Read it: the bounding box is 2 columns × 20 rows = **exactly 40 pixels, so every
pixel inside it differs.** A solid block, not scattered edge pixels. Twenty rows
is about one text line's band; two columns is about one glyph stem. Magnitude
149 is most of the way to full contrast — ink against page, not a shade of
antialiasing.

⛔ **This is NOT #69's signature, because #69 has no recorded signature** (see the
correction at the top). It cannot be matched against anything. It is a new
observation in an experimental configuration that does not ship.

### 4d. ⭐ The step-1 rubric has a gap, and this fell straight into it

The old §"What to do at the next occurrence" said:

> If it is a handful of pixels on glyph edges → atlas packing, hypothesis
> confirmed. If it is a contiguous block or a whole row band → geometry, and
> this is a real regression.

**A solid 2 × 20 block is neither.** It is far too coherent for "a handful of
edge pixels" and far too small for "a block or a row band" in the sense meant —
that phrasing was reaching for *geometry moved the whole layout*, and 40 pixels
is one glyph. The dichotomy has no bucket for **one glyph-sized region entirely
wrong**, which is exactly the shape a sample from the wrong atlas location would
make. Rubric replaced in §5.

---

## 5. ⛔ WITHDRAWN — What to do at the next occurrence

⛔ **Do not use this rubric.** It says a contiguous band means the layout moved
and is "a real regression, the most serious outcome". The contiguous band was
the caret. Applying this to a 40-pixel two-column band would produce a
confident wrong answer, which is worse than no rubric. Kept only so the §10
correction has something to point at. **See §10e.**

Steps 2 and 3 survive and are worth keeping: record the full line verbatim with
its population, and do not loosen `assert_same_frame` to a tolerance.

1. **Read the diagnostic and classify by three questions, not two:**
   - **Is the differing set larger than one glyph cell?** A row band, a column
     band, or a region spanning multiple lines → **geometry**. The layout moved.
     This is a real regression and the most serious outcome.
   - **Is it one glyph-sized region, solid?** → **atlas sampling.** A wrong
     texel selection, most plausibly after a growth rescaled `dim` (§3). Not a
     retained-path logic error, but not benign either — it means rendering is
     sensitive to atlas history.
   - **Is it a scattered handful on glyph boundaries?** → **atlas rounding at
     edges.** The mildest reading, and the only one the old rubric had.
2. **Record the full line verbatim, with its population.** Not a paraphrase.
   This document exists because a paraphrase of an *illustration* became the
   evidence.
3. Only then decide the fix. Do **not** loosen `assert_same_frame` to a
   tolerance first — a tolerance hides the geometry case, which is the case
   worth catching.

## 6. ✅ CLOSED — the load caveat

The previous sitting recorded 160 runs and warned, correctly, that load never
exceeded 5.6 while the report says *"under box load"*: *"the condition was not
reproduced because the condition was not achieved."*

**The condition has now been achieved, without manufacturing it.** The 100 runs
in §4b executed while the box carried a 1-minute load average between **48 and
112** — an order of magnitude above every previous attempt, and above the 36–51
seen when the flake was first noticed.

⭐ **The shipped test failed 0 of 50 at that load.** The "not reproduced" count
now stands at **210 executions**, and 50 of them are no longer evidence about an
idle box. That is a materially stronger negative than the previous sitting could
report — and it still is not a fix, because 0 of 50 does not bound a rate this
low.

## 7. The experiment source, so §4b is reproducible

Kept here because the test itself was **reverted, not committed** — it composes
a configuration the product does not have, and a permanently-green diagnostic in
the suite is a liability. Paste into
`crates/iridium-editor/tests/retained_shaping/gutter.rs` to re-run.

```rust
#[test]
fn experiment_69_maximised_atlas_divergence() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;
    let hashes: Vec<String> = (0..201).map(|line| format!("#{line}")).collect();
    let custom: Vec<String> = (0..201).map(|line| format!("+{line}")).collect();

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    warm.set_custom_gutter_lines(Some(hashes));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    warm.set_custom_gutter_lines(Some(custom.clone()));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut ActiveLanguageNoSpans, |fresh| {
        fresh.set_custom_gutter_lines(Some(custom.clone()));
    });
    assert_same_frame(&after, &cold, WIDTH, "maximised atlas divergence must still land byte-identical");
}
```

Paired harness: alternate this test and the shipped one, N times, keeping each
failing log — a shared run count is what makes the two columns comparable.

## 8. Not done, deliberately

No change to the test, the harness, or the assertion. There is nothing to fix
until the mechanism is known. Changing a test that cannot be made to fail is how
a flake becomes a silently disabled check.

⚠️ **And nothing here is a fix for #69.** What this sitting delivers is: one
invented figure removed from the evidence, one dead experiment deleted, one live
experiment recorded with its single observation and its honest statistics, the
load caveat closed, and a rubric that can classify what was actually seen.

## 9. ◐ SUPERSEDED — AN OCCURRENCE, AND THE RUBRIC IN §7 SAYS IT IS NOT ATLAS PACKING

⛔ **Both candidate mechanisms below are wrong, and one premise is factually
wrong.** The occurrence is real and its diagnostic is the most valuable thing
in this file — it is the caret, and §10 shows why to the pixel. What is wrong:
neither "a gutter-toggle regression" nor "contention among concurrently running
GPU binaries" is the cause, and `cargo` does **not** run test targets in
parallel, so the fourth binary #90 added never joined a "parallel run". The
section is kept intact because its *method* — record two candidates and the
separating experiment rather than a conclusion — is what produced the answer,
and §9a's transferable rule is what pointed the experiment at the right thing.

**12 Aug 2026.** Seen during the L1c gate run, in a `scripts/ci.sh` pass that
also failed `left_inset::the_content_column_is_the_same_pixels_a_sidebars_width_across`
and both gutter rows. Every one of them is a pixel-identity comparison; every
one was green on an immediate re-run with nothing changed.

⚠️ **Read the diagnostic before reading the conclusion.** From
`a_gutter_toggle_misses_and_recomposes_identically`, verbatim:

```
the gutterless frame must be byte-identical
  40 of 196608 pixels differ; the first at column 10, row 10;
  they span columns 10..=11 and rows 10..=29;
  the largest single channel difference is 149
```

That is a **contiguous 2×20 band** — two columns wide, twenty rows tall, which
at `FONT_SIZE` 14 and a 1.5 factor is one line's full height. Magnitude 149 on a
single channel is a stroke drawn against a stroke absent, not an edge sampled
differently.

§7's rubric was written to decide exactly this: *few pixels on glyph edges =
atlas packing; contiguous block/row band = real regression.* This is the second
shape. It is also **not** the recorded signature this document was built around
— that one was 1 pixel, 1 channel, magnitude 23 — so there are now two distinct
failure shapes filed under one number, and treating them as one is how the
smaller one keeps hiding the larger.

⚠️ **What this does NOT establish.** The run that produced it also produced
three other pixel-identity failures at once, which is a pattern no single-test
mechanism explains. Simultaneous failure across independent binaries points at
something shared — device contention, or an adapter under load — and the
already-recorded caveat is that all 160 clean runs were at load ≤ 5.6. This run
was not: it was the second full gate pass in a session that had just built three
crates, and #90 L1c added a fourth GPU test binary
(`run_weight_reaches_the_glyphs`, three contexts) to the same parallel run.

So the honest reading is **two candidate mechanisms, not one conclusion**:

1. A real gutter-toggle regression, which the band shape fits and which the
   rubric says to believe.
2. Contention among concurrently-running GPU test binaries, which the
   simultaneity fits and which the single-test rubric cannot see at all,
   because it was written to classify one test's pixels.

**The experiment that separates them, and it is cheap:** re-run the same gate
pass with `--test-threads=1`, or with the four GPU binaries serialised. If the
band survives serialisation it is (1) and the rubric holds. If it vanishes, the
subject of #69 was never one test — it is the harness running GPU work in
parallel, and every pixel-identity test in the tree shares the defect.

⛔ Still no change to any test, harness or assertion, for the reason §8 gives.

### 9a. Kinship: this is a serialization-dependence audit

⭐ Waffles named the family on 12 Aug: the branch where the band vanishes under
serialisation is the **same shape** as beamr's serialization-dependence audit
(Meridian task #56 in that lane) — a result that looks like a property of the
thing under test and is actually a property of how the test was run. Naming it
here so the two lanes share the method rather than each inventing one.

The transferable method, stated once: when a comparison fails intermittently
and more than one independent comparand fails together, the shared execution
context is a candidate mechanism with at least as much standing as anything
inside the test. Vary the execution context first, because that experiment is
cheap and its negative result is what licenses trusting every per-test rubric
built on top.

## 10. ⭐⭐ SOLVED — IT IS THE CARET, AND IT WAS NEVER THE GUTTER

**12 Aug 2026.** The serialisation experiment §9 asked for was run. It found the
mechanism in twelve minutes, and the mechanism is neither of §9's two
candidates.

### 10a. What was run

Three arms, not two, because the two-arm form can only discriminate by
**waiting** for a rare natural failure — and a few hundred clean runs in both
arms would have said nothing about either hypothesis. The third arm attacks the
proposed mechanism instead of waiting for it:

| arm | binaries | threads inside each |
| --- | --- | --- |
| **C** | all four launched **at once** | default |
| **P** | one after another — **the status quo** | default |
| **S** | one after another | `--test-threads=1` |

Arms alternate order per iteration (`C,P,S` then `S,P,C`) so no arm sits at a
fixed position and a load excursion cannot land on one arm. The four binaries
are `retained_shaping`, `top_inset`, `left_inset`,
`run_weight_reaches_the_glyphs`. Harness kept at
`scratchpad/serialisation-experiment.sh` for the session; it is reproduced by
its own comments, and nothing in it is piped.

⚠️ **A premise of §9 was wrong and is corrected here.** §9 wrote of "the
harness running GPU work in parallel" and of #90 L1c "adding a fourth GPU test
binary to the same parallel run". `scripts/ci.sh` runs its gates one at a time,
and `cargo test` runs test *targets* one after another — the only real
concurrency is between the tests **inside** one binary. So the simultaneity §9
found suspicious was never concurrent binaries; it was several binaries each
hitting the same time-dependent condition during one loaded pass.

### 10b. The result, before any fix

**87 binary runs, 8 failures, in all three arms**, at one-minute load averages
between 22 and 102:

| arm | runs | failures |
| --- | --- | --- |
| C | 28 | 2 |
| P | 28 | 2 |
| S | 31 | 4 |

⭐ **Failures in the fully serialised arm kill the contention hypothesis
outright.** The first three occurrences were *all* in arm S. The arm does not
matter; the load does.

**Fourteen distinct tests failed, across all three binaries** — including the
plain-versus-plain *control* in `run_weight_reaches_the_glyphs`, which exists
precisely so that a difference can be attributed:

```
document_identity::switching_back_to_the_first_document_is_a_miss_as_well
document_identity::a_second_document_through_the_same_compositor_is_not_a_cache_hit
layout::loading_a_font_misses_and_recomposes_identically
theme::a_theme_flip_misses_and_recomposes_identically
syntax::a_syntax_toggle_misses_and_recomposes_identically
syntax::a_syntax_theme_borrow_misses_and_recomposes_identically
syntax::a_language_less_source_composes_the_plain_frame
syntax::a_highlight_generation_bump_misses_and_recolors_identically
hits::steady_frames_hit_and_reproduce_the_cold_frame_exactly
gutter::custom_gutter_lines_miss_and_recompose_identically
gutter::a_gutter_toggle_misses_and_recomposes_identically
the_content_column_is_the_same_pixels_a_sidebars_width_across
a_plain_run_draws_what_an_unstyled_run_always_drew
a_bold_run_draws_different_glyphs_than_a_plain_one
```

**Every single failure carried one signature.** Verbatim, three of them:

```
40 of 196608 pixels differ; the first at column 10, row 10; they span columns 10..=11 and rows 10..=29; the largest single channel difference is 149
40 of 196608 pixels differ; the first at column 61, row 10; they span columns 61..=62 and rows 10..=29; the largest single channel difference is 149
40 of 196608 pixels differ; the first at column 67, row 10; they span columns 67..=68 and rows 10..=29; the largest single channel difference is 158
```

Always **40 pixels**. Always **two columns wide**. Always **rows 10..=29** —
twenty rows, one line. Only the column moves, and it moves with the cursor.

### 10c. The mechanism

`render/compositor/quads.rs::build_cursor_quads` emits, for every caret:

```rust
Quad::new(x, y, 2.0, m.line_height, cursor_color)
```

**Two pixels wide by one line tall.** At the harness's `FONT_SIZE` of 14 and a
1.5 line factor that is 2 × 20 = **40 pixels**, at the caret's column, on the
caret's line — `top_inset` 10 puts line 0 at rows 10..=29. The signature is not
*like* a caret. It **is** the caret, to the pixel.

Whether it is drawn at all is decided one line earlier:

```rust
if !self.cursor_renderer.is_visible() { return; }
```

and `is_visible` is a function of the wall clock. `compose` calls
`self.cursor_renderer.update(Instant::now())`; `CursorRenderer::update` derives
the phase from `elapsed % blink_interval`, with `blink_interval` one second.
**Any two composes that straddle the 500 ms half-interval differ by exactly
those 40 pixels — in any test, whatever that test believed it was comparing.**

### 10d. Why the existing mitigation did not hold — and the law

`tests/support/frame.rs` already called `compositor.reset_blink()` before every
compose, under a comment that names the mechanism exactly: *"Without it the
caret's phase depends on wall-clock time, so an otherwise identical pair of
frames differs in a handful of pixels, intermittently."* Someone saw this.

It was not enough, because **a reset is time-*relative***. It sets the cycle's
origin to *now* and buys visibility for the half-second that follows. The phase
is still recomputed from `Instant::now()` when `compose` runs. On an idle box
the gap between those two lines is microseconds and the caret is always
visible; under load it crosses 500 ms, and the caret vanishes from one frame of
a comparison.

⭐ **A guard whose correctness depends on the box being fast is a guard that
works when it is not needed and fails when it is.** This is a sibling of *"a
fallback the real failure mode cannot reach is not a fallback"*: the mitigation
was reachable only in the conditions that never needed it. The 210 clean runs
in §4b and §6 are explained — they were runs in which the mitigation held.

### 10e. What this retires

⛔ **§5's rubric and §7's classification are withdrawn.** They said: *few
pixels on glyph edges = atlas packing; contiguous block or row band = a real
regression, and the most serious outcome.* The contiguous band was a caret.
Any future reader applying that rubric to a 40-pixel two-column band would
reach a confidently wrong conclusion.

⛔ **The atlas hypothesis (§3) is not confirmed by anything.** It remains
neither proved nor disproved — but it now has **no** observation behind it: the
one occurrence §9 offered it, and the earlier signature §0 corrected, are both
accounted for by the caret.

⭐ **And this is the branch §9a warned of, in its more expensive form.** Not
"one test's rubric was wrong" but "every pixel-identity comparison in the tree
shared one confound, and the document was calibrated on it". The kinship named
in §9a holds and the method paid: **vary the execution context first.** What
made it pay was widening "execution context" past the harness's threading to
include *the clock* — which is shared execution context too, and the only one
the failures were actually sensitive to.

### 10f. The fix

`FrameCompositor::set_cursor_blink_enabled(bool)` — new, and
product-legitimate: a solid caret is a real setting, not a test hook. **Off
means solid, not hidden.** The caret is still drawn on every frame at its real
position, so a caret-placement regression is still caught; only the phase — the
part that makes a frame a function of when it was drawn — is removed.

Pinned off in five places, each with the reason at the call site:

- `tests/support/gpu.rs::compositor_sized` — every kernel harness compositor,
  from birth.
- `tests/support/frame.rs::compose` — again, per compose, because that function
  accepts *any* compositor including one a test built for itself, and a
  guarantee that depends on the caller having remembered is not one.
- `apps/iridium-desktop/tests/plain_frames.rs` — same comparison, same defect.
- `apps/iridium-desktop/tests/chrome_screenshots.rs` — which had no mitigation
  at all, so its shots showed a caret or not at random.

⭐ **Pinning it off in the harness does not make the harness diverge from the
product, and that is worth stating because it is the obvious objection.** The
desktop face composes only on `RedrawRequested`, and requests one only after
input, resize or scale change — there is no animation loop
(`apps/iridium-desktop/src/app/mod.rs`, which names the consequence in its own
words: *"the caret does not blink while the keyboard is idle"*). So the shipped
editor already draws a solid caret except across an input. The phase existed
only where frames are composed back to back, which is exactly and only the test
harness.

Two instruments, so the finding cannot decay into a comment:

- `render::cursor::tests::a_disabled_blink_is_visible_at_every_moment_an_enabled_one_would_hide`
  — swept over two full intervals rather than sampled at one instant, so it is
  deterministic whatever the cycle's origin and whatever the load. Determinism
  is the property under test, so the test must not itself be timing-dependent.
- `support::gpu::a_harness_compositor_draws_a_caret_that_does_not_depend_on_the_clock`
  — runs in **every** binary that pulls the harness in. The guarantee belongs
  to the harness, not to whichever test remembered to ask.

### 10g. The after column

Same harness, same three arms, same box, at a comparable load. Recorded on the
next line rather than promised:

| arm | runs | failures |
| --- | --- | --- |
| C | 52 | **0** |
| P | 52 | **0** |
| S | 51 | **0** |

**155 binary runs, 0 failures** — a sample 1.8× the size of the one that
produced 8 — over 155 load samples between **39.74 and 55.86**, mean 49.39.

⚠️ **That first after-run had a real gap, and it is recorded rather than
quietly dropped.** The before-run's load ranged from 22 to 102 and **five of its
eight failures were above 60** — outside the band this run reached. Waffles
named it on sight: *"if you can push an arm above 60 before committing, the
claim closes fully rather than mostly."* So it was pushed.

### The high-load arm — the caveat closed

The box went to a one-minute load average above **400** shortly afterwards
(other lanes, plus a browser, not this work). The same harness was run again
against **copies** of the fixed binaries in a scratch directory — deliberately,
so that the `scripts/ci.sh` pass running concurrently could not rewrite a binary
mid-execution and turn a failed `exec` into a counted "failure".

| arm | runs | failures |
| --- | --- | --- |
| C | 100 | **0** |
| P | 100 | **0** |
| S | 100 | **0** |

**300 binary runs, 0 failures**, over 300 load samples from **49.83 to 433.03**,
mean **123.34**.

⭐ **That is four times the load at which the flake originally fired, and it
spans the entire before-run band rather than overlapping part of it.** Combined:
**455 post-fix binary runs, 0 failures, across loads 39 to 433**, against **8
failures in 87 runs** before. The caveat is closed.

The proof is still not the counting. The proof is that the caret quad is
2 × `line_height`, the signature was 2 × 20 every single time, and the column
moved with the cursor. The counts are what rule out a second mechanism hiding
behind the first.

### 10h. Still open, honestly

- The **atlas** question (§3) has no evidence for or against it. It should not
  be closed; it should be left with nothing behind it, which is what it has.
- This says nothing about whether a *different*, rarer mechanism also exists.
  It says the mechanism behind every failure this experiment produced, and
  behind the one occurrence §9 recorded, is the caret.
- ⚠️ The desktop screenshot corpus was generated with a live blink. Any shot in
  it may or may not show a caret. Regenerating it is not in this lane.
