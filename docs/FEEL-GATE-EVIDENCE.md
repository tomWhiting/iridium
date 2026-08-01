# Step 7 evidence — measured, with the instrument described

2026-08-01, release binary `7f1a5bc` (14,088,720 bytes), driven over a `script`
pty sized by `COLUMNS=120 LINES=40`, on the shared box while otherwise idle.
Every number below was produced by a command whose keys were **proven delivered
by artifact** — a navigation, an edit, and a save whose effect was read back out
of the file afterwards — because the first version of this harness silently
delivered nothing and timed an app dying on a 0×0 pty.

## What a pty can and cannot measure

A pty with a discarded slave measures **compute**: load, layout, paint, diff,
teardown. It does not measure what a human sees — no real terminal is drawing,
so vsync, terminal-emulator rendering and display latency are all absent.
The keystroke-to-glyph half of the feel gate is currently being run the honest
way: by Tom, by hand, in a real terminal. These numbers bound the editor's own
contribution to that latency.

## Results

Fixed sleeps in the pipe (0.6s open runs, 0.9s flood runs) dominate wall time;
the editor's cost is the excess over them. Three runs each.

| Scenario | wall (3 runs) | editor cost |
|---|---|---|
| open + paint + quit, empty buffer | 0.623 / 0.619 / 0.621 | ~20 ms |
| same, 8-line JSON | 0.621 / 0.622 / 0.617 | ~20 ms |
| same, **100,000-line / 9.4 MiB JSONL** | 0.622 / 0.626 / 0.619 | ~21 ms |
| 400 Down-arrows through the 100k file, then quit | 0.974 / 0.978 / 0.957 | ~70 ms |

* **Opening 100k lines costs ~1 ms over an empty buffer** across the whole
  open→parse→paint→quit round trip. The rope and the viewport-only paint are
  doing their jobs.
* **400 keystroke-repaints ≈ 50 ms of compute ≈ 0.13 ms per repaint** at
  120×40 on the 100k file. The 5 ms/keystroke budget has two orders of
  magnitude of headroom *in compute*.
* The whole editor round trip (~20 ms) sits inside the 50 ms startup budget
  with the load and first paint included.

## Proven by artifact, not by exit code

| Claim | Artifact |
|---|---|
| goto 100000 works | `100000:PROOF{"id": 100000, …` read back from the saved file |
| every arrow in a 400-key flood lands | `401:MARK{"id": 401, …` — exactly 400 lines down |
| the atomic save writes 9.4 MiB correctly | saved torture file, 100,000 lines, edited line correct |
| a dirty buffer refuses a bare quit | a flood that accidentally typed `MARK` hung the harness on the quit prompt — the guard held against a robot |

## Defect found by the torture run — FIXED at `60eae56`

**`PageUp` / `PageDown` were bound to nothing, in every face.** The kernel's
`KeyCode` carried them, the TUI mapped them faithfully, the default keymap never
bound them, and the keyboard handler had no page-motion action to bind. Proven
end-to-end: three PageDowns from the top of the 100k file left the caret on
line 1 (`1:MARK…`).

Fixed with standard semantics — `cursor.pageUp`/`cursor.pageDown` + Shift-select
variants as kernel verbs, the hop one viewport-height with sticky columns kept,
bound to the page keys in the default keymap so every face inherits them.
Re-proven by the same harness on the same file: three PageDowns at `LINES=40`
marked **line 118** in the saved artifact — 3 × 39 rows, the pty's 40 minus one
row of chrome. The scroll-without-caret variant some editors also offer remains
a separate open question with Tom; the browser face additionally needs its host
to sync the viewport height (`KeyboardHandler::set_page_rows`) before its pages
are more than one line.

## Harness traps, for whoever measures next

1. **`script`'s pty is 0×0 when stdin is a pipe.** The driver refuses it
   (`cannot read non-zero cols/rows…`). Set `COLUMNS`/`LINES`. Every timing
   taken before this was discovered measured an app failing to start — and
   looked plausible.
2. **Bytes written before raw mode are eaten by the line discipline.**
   Ctrl+S/Ctrl+Q are XOFF/XON; with `IXON` still on they vanish as flow
   control. Sleep before sending.
3. **Prove delivery with an artifact** — navigate, edit, save, read the file
   back. An exit code proves something exited; `401:MARK` proves 400 arrows
   were processed.
