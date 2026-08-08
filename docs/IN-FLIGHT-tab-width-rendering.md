# #86 — the configured tab width reaches the renderer

Tom ruled 8 Aug: wire it up. Found by the Rule E pass on
`FrameCompositor::compose`.

## The defect

`EditorConfig::tab_width` is settable from `[editor]` in the configuration
file. It is read by **one** module — `input/keyboard/behaviors.rs` — for the
indent string, the unindent width, and one level of indentation. Nothing under
`crates/iridium-editor/src/render/` reads a tab width at all, so a literal tab
character measures at cosmic-text's default of **8** whatever the file says.

Reachable, not theoretical:

- `insert_spaces = false` is settable (`[editor]` deserializes into
  `EditorConfig`, which derives `Deserialize` with `#[serde(default)]`), so
  Iridium itself will insert literal tabs on request.
- Any file opened off disk may hold tabs regardless — Go, Makefiles, much C.

So `tab_width = 2` gives two-column *unindent* and eight-column *tabs on
screen*. One setting, two answers.

## The shape of the fix

cosmic-text 0.15 keeps tab width **per `Buffer`**:
`Buffer::set_tab_width(&mut self, font_system, tab_width: u16)`. It ignores a
zero, and re-shapes only when the value actually changes. The advance is
`tab_width × glyph.x_advance` (`shape.rs:1014`) — a multiple of the character
advance, which is what a monospace editor wants.

1. **`TextRendererConfig` gains `tab_width: u16`**, beside `font_size` and
   `line_height`, and `TextRenderer::create_buffer` applies it to every buffer
   it makes. That is the single construction point, so no buffer can be born
   without it.
2. **`ShapeKey` gains `tab_width: u16`.** This is the load-bearing half. The
   compositor skips shaping entirely on a key hit, so without a key member a
   configuration reload would re-measure nothing and the retained buffers would
   keep the old columns — exactly the staleness the retained-shaping map warns
   about, and the reason row 11 named this as its second trigger.
3. **`permits_line_diff` requires equality on it**, so a tab-width change takes
   the full-rebuild branch (a fresh `create_buffer`) rather than the per-line
   diff branch (which reuses the previous buffer and would keep its old tab
   width). This is why the field must be named in that destructuring, not just
   in the key.
4. **`compose` reads `editor.get_config().tab_width`** — it already takes
   `&Editor` — floors it at 1 and saturates into `u16`.

## Why floor at 1 rather than let cosmic-text ignore a zero

`behaviors::tab_width` already floors at 1, with a stated reason: a zero from a
hostile or corrupt file must not produce an empty indent level. cosmic-text
instead *ignores* a zero and leaves the buffer at its previous width (8 by
default).

Take the two together and `tab_width = 0` would mean **one** column when
indenting and **eight** when rendering — a fresh instance of the very
divergence this task exists to close. Flooring at 1 in both places makes the
two agree on a value neither would have chosen, which is the point: they agree.

## Verification

Red first. The oracle is that a document containing a literal tab shapes to a
different width when `tab_width` changes, and that the retained cache does not
serve a stale buffer across that change — the second is the part a naive fix
gets wrong, and it is why the test asserts on the reshape rather than only on
the glyph geometry.

⚠️ The GPU-backed compositor tests need a device; the `ShapeKey` half is
assertable without one.

---

# Landed — 8 Aug 2026

**Gates: all nine green. 2,545 / 1,061 / 1,157, 0 failed.** Gate 1 is up four
on 2,541 — exactly the four new tests, nothing else moved.

## What changed

- **`render/text.rs`** — `TextRenderConfig::tab_width`, `DEFAULT_TAB_WIDTH`
  (8, cosmic-text's own, restated so the shipped value is visible here),
  `usable_tab_width`, `TextRenderer::tab_width`/`set_tab_width`, and one line
  in `create_buffer`. That last is the whole rendering half: `create_buffer` is
  the single place a rendered buffer is born, so no buffer can miss it.
- **`compositor/shape.rs`** — `ShapeKey::tab_width`, the refusal in
  `permits_line_diff`, and four tests.
- **`compositor/frame.rs`** — `compose` floors the configured width once and
  uses that one value for both the renderer and the key.

## The two-places trap

The key member alone does **not** fix it. A key miss routes through
`permits_line_diff`, and that path reuses the *previous* buffer, which carries
the tab width it was constructed with — so diffing text into it leaves every
tab at the old width with no line marked dirty. Both the key member and the
refusal are needed. `a_tab_width_change_refuses_the_line_diff` is what holds
the second.

**Proven red** by deleting that one comparison: exactly that test failed, with
its own message, while three controls stayed green — an unchanged key, a
content-only change (the case the fast path exists for), and the floor. The
controls are the point: without them the assertion would pass against a
`permits_line_diff` that refused everything.

## Two things the gates caught that reading did not

1. **`--no-default-features` does not compile `render/`.** The first red run
   filtered to `shape_key_tests` and found *nothing*, reporting a cheerful exit
   0. A red run that finds no test is not a red run. Re-run under
   `--all-features`, where it failed properly.
2. **`items_after_test_module`.** The test module was placed before
   `RetainedShape`, and clippy refused it. Moved to the end of the file.

## What this does not close

⚠️ **The flooring is pinned; the buffer wiring is not.** `usable_tab_width` has
a direct test, and `ShapeKey` has four. But nothing asserts that
`create_buffer` actually calls `set_tab_width` — `TextRenderer` needs a GPU
device, so that assertion belongs with the existing readback harness
(`crates/iridium-editor/tests/`), not in a unit test. The gap is narrow: one
line, at the single construction point, in a function whose other two lines are
exercised by every GPU test that renders anything.

⚠️ **The web face is untouched.** `WebEditor` drives its own compositor, and
nothing plumbs a configured tab width into it — a browser has no configuration
file to read (`iridium-config` is native-only, and says so). If tab width ever
needs to reach the browser it wants a wasm export, like every other setting.
