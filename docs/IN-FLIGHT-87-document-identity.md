# #87 — one compositor, two documents, and the wrong file on screen

**Found 8 Aug 2026, while probing #69. Reproduced, not inferred.** It is not
#69 — that is a one-pixel difference under load — but the probe is what turned
it up.

## The defect

`ShapeKey` identifies a document by **`Document::revision`, and nothing else**.
`Document::new` sets `revision: 0` (`document/buffer.rs:117`) and it counts text
mutations from there. So it is a *change* counter, not an *identity*: two
different documents that have been mutated the same number of times carry the
same revision.

`FrameCompositor::compose` takes the `&Editor` as an argument. A face holds one
compositor and composes whichever document is active — `DesktopApp` does exactly
that, `app/paint.rs:80`, with `editor` coming from the active tab. So switching
tabs composes a different document into the same cache.

**When every other key member happens to agree, the switch is a cache hit and
the compositor re-presents the previous document's shaped buffers.** The screen
shows the wrong file's text.

## Reproduced

`crates/iridium-editor/tests/retained_shaping/document_identity.rs`:

```
revisions: first=1 second=1
shape_rebuilds after both = 1
the shared compositor drew the FIRST document's text for the second
```

Two documents, three lines each, one of `A`s and one of `Z`s, both loaded with
`set_content` so both sit at revision 1. One compositor. The second compose is a
**hit** — `shape_rebuilds` never leaves 1 — and its pixels equal the *first*
document's frame rather than a cold compose of the second.

## What has to agree for it to fire, and how easily it does

Every other member of the key. Going through them for a tab switch between two
files freshly opened from disk:

| member | agrees? |
| --- | --- |
| `document_revision` | **yes** — both at 1 after one `set_content` |
| `viewport_start` | yes — both at the top |
| `viewport_end` | **yes whenever both files are longer than the window**; differs only if one is shorter, which is the accidental save |
| `content_width`, `font_size`, `line_height_factor` | yes — one window |
| `font_generation`, `theme_generation`, `syntax_theme_generation` | yes — compositor-wide |
| `syntax_enabled` | yes |
| `language_active` | yes when both files are the same language |
| `highlight_generation` | per-document, both parsed once |
| `fold_generation` | per-document, neither folded |
| `gutter_text_generation` | yes |
| `tab_width` | yes — one configuration |

So: **two files of the same language, both longer than the window, opened and
not yet edited.** That is not a corner, it is the ordinary case.

⚠️ It also fires *after* editing, whenever two documents' edit counts coincide —
which they do constantly, since both start at 0 and both count.

## The fix, in the shape the codebase already uses

A document needs an **identity** distinct from its revision, and the key needs
to carry it. Everything else in `ShapeKey` that answers "did this change" is a
generation, so:

1. **`Document` gains an id** assigned at construction from a process-wide
   `AtomicU64`, never reused, never derived from content. Not a hash — two
   identical files are still two documents, and a face switching between them
   must reshape because the *buffers* it retained belong to the other one.
2. **`ShapeKey` gains `document_id`**, and `permits_line_diff` must **refuse**
   on it. This is the same two-places trap as `tab_width` in #86: a key miss
   routes through the per-line diff, which reuses the *previous* buffer, so
   diffing the new document's text into the old document's buffer would keep
   every line the diff did not touch.
3. **The red test stays**, as `document_identity.rs`.

⚠️ **`viewport_end` is the reason this hid.** It is derived from the line count,
so the two documents most likely to be told apart by accident are two of
*different lengths and both short* — which is exactly the shape a small test
fixture has. Every existing retained-shaping test composes one document per
compositor, so none of them could have caught it.

## Where else the same shape may reach

Not yet checked, and named so it is not forgotten:

- The **web face** drives its own compositor from `WebEditor`; `setContent`
  replaces the document in place, which moves the revision, so it is probably
  safe — but `WebEditor` is one editor per canvas anyway.
- The **highlight cache** is keyed to the compositor's seam
  (`app/state.rs:51`). If it identifies a document the same way, it has the
  same defect. **Check this before calling #87 closed.**

---

# ▶ IN FLIGHT — written at a context ceiling, 8 Aug ~16:10

**Nothing is committed. The working tree carries a half-built fix.** Below is
everything needed to finish it.

## Done in the tree, uncommitted

1. **`document/buffer.rs`** — `Document` gained `id: u64`, a
   `static NEXT_DOCUMENT_ID: AtomicU64`, `fn next_document_id()`, and
   `pub const fn id(&self)`. `new` takes an id; `continuing_from` takes a
   **fresh** one (documented why). Imports `std::sync::atomic::{AtomicU64,
   Ordering}`.
2. **`render/compositor/shape.rs`** — `ShapeKey::document_id` as the first
   field; `permits_line_diff` destructures it and **requires equality** (the
   two-places trap from #86); the `shape_key_tests` fixture sets
   `document_id: 1`.
3. **`render/compositor/frame.rs`** — `document_id: doc.id(),` in the key.
4. **`tests/retained_shaping/document_identity.rs`** — the red test, and
   `main.rs` declares `mod document_identity;`.

## What is LEFT to do, in order

1. **Rewrite `document_identity.rs` properly.** It is currently the raw probe:
   it prints revisions and `shape_rebuilds` with `println!`, and its name is
   `two_documents_through_one_compositor_render_as_themselves`. Keep the
   assertions — they are right — drop the prints, and add the second half:
   assert `shared.shape_rebuilds() == 2` after both composes, which is the
   direct statement that the second document was a *miss*.
2. **Add a `permits_line_diff` unit test** in `shape.rs`'s `shape_key_tests`,
   beside `a_tab_width_change_refuses_the_line_diff`:
   `a_document_change_refuses_the_line_diff`. Same argument, same shape.
3. **⚠️ Re-prove red.** The test was red *before* the fix — captured output:
   `revisions: first=1 second=1`, `shape_rebuilds after both = 1`, and
   `the shared compositor drew the FIRST document's text for the second`.
   Re-prove it by removing `document_id` from the key (or from
   `permits_line_diff`) once the test is in its final form.
4. **Run the full nine-gate battery.** Expect the count to move by the new
   tests only. Last green before this work: **2,554 / 1,068 / 1,164**.
5. **Check the highlight cache** — `app/state.rs:51` says it "is keyed to the
   compositor's seam". If it identifies a document by revision too, it has the
   same defect. **#87 is not closed until this is checked.**
6. Update `docs/design/RETAINED-SHAPING-MAP.md` — the input inventory gains a
   row, and `ShapeKey`'s doc comment claims completeness against it.

## ⚠️ The claim to re-check before believing the fix is enough

`Document::clone()` keeps the id, deliberately (one clone site,
`input/keyboard/editing/mod.rs:386`, a scratch copy for computation). If any
face ever clones a document to make a *second editable* one, it would share the
identity and the bug returns. Nothing does that today. Named so it is checkable.

## How this was found, because the route matters

Chasing #69, the probe composed twenty different documents into one compositor
to perturb its glyph atlas — and asserted its own premise, that the rounds
actually rasterise different glyphs. That assertion failed: **1 of 20 frames
differed from the one before**. The probe was not weak; the compositor was
serving hits across twenty documents.

⭐ **The rule: assert the premise of a probe, not just its conclusion.** Had it
only checked the conclusion — "the frames still match, so atlas state does not
reach the output" — it would have reported a clean negative result and this
defect would still be live.

## #69 itself is untouched and still open

Separate defect: a one-pixel, magnitude-23 difference under box load, never
reproduced. The atlas hypothesis in `IN-FLIGHT-69-flaky-gutter.md` is **still
unsettled** — and note that its proposed settling experiment ("warm the cold
compositor and the two become identical by construction") **cannot settle
anything on a box where the test always passes**: they are already identical.
The experiment that could is the inverse — perturb the atlas hard and see
whether the signature appears — and that is what the probe was, before it found
something else.
