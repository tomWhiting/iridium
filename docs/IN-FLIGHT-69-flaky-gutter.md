# #69 — `custom_gutter_lines_miss_and_recompose_identically`

**Still not reproduced. The load caveat is now closed. And the signature this
doc reasoned from was never measured — it was an illustration.**

`crates/iridium-editor/tests/retained_shaping/gutter.rs:73`.

Third sitting. The first added `support::pixels::assert_same_frame` so the
*next* occurrence says count, position, area and magnitude — deliberately not a
fix. The second eliminated two mechanisms and named a third. This one ran the
experiment the second sitting proposed, found it cannot discriminate, ran its
inverse instead, and — chasing the recorded signature to compare against —
found the signature had no measurement behind it.

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

## 5. What to do at the next occurrence — revised, in order

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

## 9. ⭐ AN OCCURRENCE, AND THE RUBRIC IN §7 SAYS IT IS NOT ATLAS PACKING

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
