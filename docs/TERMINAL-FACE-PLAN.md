# The terminal face — plan

Phase 4 of `PLAN.md`. Both pre-conditions it names are discharged: D1 is
answered (see `TERMINAL-STACK.md` — modal and non-modal are one mechanism, and
the resolver already implements it), and the stack was re-verified by compiling
on 1 Aug 2026.

`TERMINAL-STACK.md` holds the verified API facts. **Do not re-derive them.**

## The shape

```text
apps/iridium          the binary: open/save, statusline, go-to-line, theme
  └── iridium-tui     the face: cell buffer, diff repaint, input adapter, driver
        └── iridium-editor   the kernel: document, history, layout, commands
```

`iridium-tui` is a library and is the only crate that knows what a terminal is.
`apps/iridium` is a thin binary over it. Neither may reimplement a verb the
kernel already has — three separate bugs this year were a face doing exactly
that (multi-cursor, `deleteToLineStart`, undo), so treat it as the standing
suspicion.

## Four decisions taken up front

**1. The terminal drives the existing `Viewport` in cell units.** The kernel
already owns fold-aware layout (`render/viewport.rs`: `visible_document_lines`,
`visible_line_range`, `screen_y_for_line`, all fold-aware). It is expressed in
`f32` pixels, so the terminal face sets `line_height = 1.0` and
`width = columns as f32`, making one cell one unit. **No second layout model.**
This is the same reason termwiz was rejected: a competing surface/damage model
would duplicate ~4,750 lines of layout the kernel already owns.

**2. A cell is not a character.** `unicode-width` governs. A double-width glyph
(CJK, most emoji) occupies two cells, the second being a continuation cell that
holds no character of its own and must never be written independently. This is
where the fixed char-width assumption dies permanently — *including in mouse hit
testing*, which currently assumes it. A grapheme cluster may also span several
`char`s in one cell. Column arithmetic that says `col += 1` per `char` is wrong
and will be treated as a bug.

**3. Damage is computed, never tracked by callers.** The face keeps a front and
a back buffer, renders freely into the back, and diffs. Nothing outside the
buffer gets to declare a region dirty — that is how flicker and stale cells
happen. The diff emits the minimum cursor-move-plus-write sequence, wrapped in
synchronized output (BSU/ESU) so a frame is atomic.

**4. Only the last input conversion is ours.** termina parses VT, then
`terminput_termina::to_terminput`, then our adapter to the kernel's `KeyPress`.
`to_terminput` returns `UnsupportedEvent` for raw DCS/CSI/OSC — those are
*capability replies*, not user input. **Do not swallow that error**; it is the
feature-detection signal, and discarding it is how capability negotiation
silently stops working.

## Order of work

Steps 1 and 2 are independent, need no terminal, and are fully unit-testable —
they go first and in parallel.

1. **Cell buffer + diff repaint.** `Cell`, `CellBuffer`, front/back, damage
   diff, `unicode-width` cell math, continuation cells, truecolor with
   256-colour and 16-colour degradation. Pure logic; no I/O.
2. **Input adapter.** `terminput::Event` → the kernel's `KeyPress`/`Modifiers`.
   Tested without a pty by using terminput's **encoder** to synthesise the exact
   bytes a real terminal sends, under both `Encoding::Xterm` and
   `Encoding::Kitty`, then parsing them back. Legacy fallback is a first-class
   path, not an afterthought: under xterm, `Ctrl+Z` and `Ctrl+Shift+Z` are the
   same byte and cannot be told apart.
3. **The driver.** termina: raw mode, alternate screen, bracketed paste,
   keyboard-enhancement push/pop, panic hook and `Drop` restore, resize, the
   blocking `poll(None)` idle loop. Ties 1 and 2 together.
4. **The frame.** Kernel visible lines + highlight spans → back buffer. Gutter,
   cursor(s), selection. Statusline.
5. **Search & replace UI.** The engine has existed for months with no UI in any
   face; the terminal gets the first one.
6. **`apps/iridium`.** Open/save with atomic write and external-change
   detection, go-to-line, theme loading, folds, the multi-cursor verb set.
7. **The feel gate.** Keystroke-to-paint < 5ms measured, 100k-line torture
   test, no flicker under fast scroll, startup < 50ms. Measured and recorded as
   an artifact, per the standing performance-truth rule — not vibes.

## Standing constraints

Everything in `CLAUDE.md` applies, and two bear repeating because a TUI tempts
both: **no `#[allow]`/`#[expect]` in non-test code**, and **no
`unwrap`/`expect`/`panic!`** — a panic with the terminal in raw mode is the
worst failure mode this face has, which is why termina's panic hook and `Drop`
restore are mandatory rather than optional.

Modules stay under ~500 lines; a directory module is the answer, never
`include!`.
