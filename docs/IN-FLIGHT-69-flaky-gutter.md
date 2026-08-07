# #69 — `custom_gutter_lines_miss_and_recompose_identically`

**Still not reproduced. Two hypotheses eliminated, one named precisely.**

`crates/iridium-editor/tests/retained_shaping/gutter.rs:73`.

This is the second sitting on it. The first added
`support::pixels::assert_same_frame` so the *next* occurrence says count,
position, area and magnitude instead of "it differed" — deliberately not a fix.
This one tried to reproduce it and failed, but eliminated two of the three
things it could have been.

## Attempts, this sitting — 148 executions, 0 reproductions

| condition | runs | fails | max 1-min load seen |
| --- | --- | --- | --- |
| the test alone, sequential | 30 | 0 | 3.23 |
| the test alone, 6 processes in parallel | 90 | 0 | 5.22 |
| the whole binary (28 tests, default parallelism) | 12 | 0 | 5.59 |
| 4 whole binaries concurrently | 16 | 0 | 5.28 |

Plus 12 from the previous sitting. **160 total, none failed.**

⚠️ **Load never exceeded 5.6, and the report says "under box load".** This box
was at **36.28** and **50.8** at points earlier in the same session. The
condition was not reproduced because the condition was not achieved, and
manufacturing that load on a shared box to chase a flake is not a trade worth
making. So these runs are evidence about the *idle* box only, and the count
should not be read as "it is fixed".

## ✅ ELIMINATED — readback synchronisation

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

## ✅ ELIMINATED — cross-test GPU state

`support/gpu.rs:103` `gpu(harness)` builds a **fresh `wgpu::Instance`, adapter
and device on every call**. No `static`, no `OnceLock`, no `thread_local`. Two
tests running concurrently share no wgpu object, so "another test perturbed
this one's device" is not available as an explanation.

It does mean `cargo test` runs many simultaneous devices — which is why the
16-run 4-way-parallel condition above was worth trying. It still did not fire.

## ⭐ THE ONE THAT SURVIVES — warm and cold hold different glyph-atlas state

The test compares:

- `after` — a compositor that has composed **twice**: once with the default
  gutter (digits `0`..`200`), then again after
  `set_custom_gutter_lines(Some(["+0".."+200"]))`.
- `cold` — a **fresh** compositor that composes once, with the custom gutter
  already set.

Those two are *legitimately different objects*. The warm one's glyph atlas has
the plain digit glyphs cached from the first compose; the cold one has never
seen them. So the two atlases hold different glyph sets, in different insertion
orders, at different texture coordinates.

**The test asserts their rendered pixels are byte-identical.**

⭐ **Stated as the proxy it is:** byte-identical pixels stand in for *"the
retained path recomposed correctly"*. They agree on every input examined so far.
**Name where they diverge:** any input for which atlas *packing* reaches the
rasterised output — a growth or repack that crosses a size threshold, or
sampling that picks up a neighbouring glyph's texel at an edge. Then the frames
differ by a pixel or two at a glyph boundary and the test fails while the
retained path was perfectly correct.

That signature matches the one occurrence on record exactly: **1 pixel of
393216, one channel, magnitude 23.** A logic error in the retained path does not
produce one pixel; it produces a region.

## What to do at the next occurrence — in order

1. **Read the diagnostic.** `assert_same_frame` now prints count, first
   position, spanned columns/rows and largest channel delta. **If it is a
   handful of pixels on glyph edges → atlas packing, hypothesis confirmed. If
   it is a contiguous block or a whole row band → geometry, and this is a real
   regression.** That single distinction is what the previous sitting bought.
2. Only then decide the fix. Do **not** loosen `assert_same_frame` to a
   tolerance first — a tolerance hides the geometry case, which is the case
   worth catching.

## The experiment that would settle it without waiting

Give the cold compositor the *same* warm-up: compose once with the default
gutter before `set_custom_gutter_lines`, so both atlases have seen the same
glyphs in the same order. If the difference is atlas state, the two become
identical by construction.

⚠️ **This is a diagnostic, not the fix.** Doing it permanently would weaken what
the test checks — the point is that a *fresh* compositor reaches the same frame,
and warming it up first is assuming the answer. Run it to learn, then revert.

## Not done, deliberately

No change to the test, the harness, or the assertion. There is nothing to fix
until the mechanism is known, and the two eliminations above narrow it to one
candidate with a stated signature. Changing a test that cannot be made to fail
is how a flake becomes a silently disabled check.
