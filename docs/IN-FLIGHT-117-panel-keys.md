# #117 — the other seven panel key tables

**Started 13 Aug 2026. Two of seven panels done. Read "Where this stands".**

## Where this stands — 13 Aug 2026

| # | panel | state |
| --- | --- | --- |
| 1 | desktop command palette | ✅ **DONE** — the split `3d491f8e`, the conversion below |
| 2 | desktop history overlay | not started |
| 3 | TUI command palette | not started |
| 4 | TUI history panel | not started |
| 5 | TUI search | not started |
| 6 | desktop context menu | not started |
| 7 | desktop menubar | ⛔ **do not touch** — #108 waits on Tom clicking a menu item |

The explorer (#91) and the command palette are both on the mechanism. Everything
below the horizontal rule was written *before* the palette landed and is kept as
the record of how it was planned; the appendix at the foot has been superseded by
real files and says so.

### What the palette conversion actually landed

**Kernel** — `commands/builtin/panel/palette.rs`: `PALETTE_MODE` and twelve
verbs, added to `TABLES` and to `builtin/mod.rs`'s re-export list.

**Desktop face** — `command_palette.rs` (811 lines) became a directory:

| file | lines | holds |
| --- | --- | --- |
| `mod.rs` | 93 | the module argument, declarations, re-exports |
| `panel.rs` | 438 | the state and the composition |
| `keymap.rs` | 127 | 16 bindings, the three patterns, `EVERY_PATTERN` |
| `resolve.rs` | 175 | `Resolved`, `resolve_key`, `set_user_keymap`, `types_text` |
| `verb.rs` | 106 | the twelve verbs, `ALL`, `id()`, `from_id()` |
| `keys.rs` | 85 | the dispatch and the printable fall-through |
| `tests.rs` | 317 | the behavioural tests, moved unchanged |
| `keymap_tests.rs` | 364 | 14 new ratchets |

`Chord` and `chord()` are **deleted**, as #91 deleted the explorer's.

### ⭐ Two mutations, both measured

| # | mutation | what failed | what did not |
| --- | --- | --- | --- |
| M1 | `PLAIN`'s shift `Any` → `Forbidden` | the 3 shift ratchets | **386 other tests passed** |
| M2 | `set_user_keymap` pushes the user layer unfiltered | `a_user_binding_naming_a_document_command_never_reaches_this_panel` | **388 other tests passed** |

M1 reproduces #91's finding on this panel exactly: **nothing but a per-panel
shift ratchet catches D-4.** M2 is the same measurement for D-2. Both ratchets
are justified by what did *not* fail, not by what did.

### ⚠️ Two things the conversion had to fix beyond the panel

1. **The explorer's `every_registered_panel_command_is_a_verb_the_panel_answers`
   would have broken.** It walked `panel_command_metas()` whole, which now
   carries another panel's twelve verbs. Narrowed to the explorer's four modes,
   with a `seen == Verb::ALL.len()` guard so a renamed mode cannot make the
   filter vacuous instead of failing.
2. **The 16 new chords would have been absent from `config.toml`** — regressing
   Tom's #118 ask on the day after it landed. The palette's layer lives in the
   face, unreachable from `iridium-config`, so `commands/template.rs` now hands
   over a **second `FaceKeys`** entry. Verified by reading the generated file,
   not only by the test.

📌 **A panel converted without its `FaceKeys` entry is a panel whose keys are
undiscoverable.** Every remaining conversion owes one.

---


#91 converted the file explorer. Seven panels still answer keys from hard-coded
`match (Chord, KeyCode)` tables and cannot be rebound:

| face | file | lines |
| --- | --- | --- |
| desktop | `apps/iridium-desktop/src/command_palette.rs` | 811 |
| desktop | `apps/iridium-desktop/src/context_menu.rs` | 707 |
| desktop | `apps/iridium-desktop/src/menubar.rs` | 426 |
| desktop | `apps/iridium-desktop/src/history_overlay.rs` | 525 |
| terminal | `crates/iridium-tui/src/frame/search/mod.rs:242` | |
| terminal | `crates/iridium-tui/src/frame/history_panel/mod.rs:103` | |
| terminal | `crates/iridium-tui/src/frame/command_palette/mod.rs:147` | |

The mechanism is built and proven. This is application, not design — see
`docs/IN-FLIGHT-91-panel-keys.md` for the shape.

---

## ⛔ CORRECTION TO THE SECTION BELOW — read this first

The plan below said the palette *vocabulary* was in the working tree. **It was
taken back out**, for a reason worth recording.

`git mv` stages the rename immediately, so committing the in-flight doc also
committed `panel.rs` → `panel/explorer.rs` **without** the `panel/mod.rs` that
makes the module tree valid — a HEAD that did not compile, created by a command
that looked like it only touched a document.

📌 **`git mv` is not a filesystem move. It stages.** A later `git commit` of an
unrelated path carries it.

The fix chosen was to land the **split only** — structural, no behaviour change,
no new vocabulary — rather than to commit twelve registered `palette.*` commands
that nothing resolves. So:

- `panel/mod.rs` declares `explorer` alone; `TABLES` has one entry.
- **`panel/palette.rs` is written and correct, and is parked at**
  `<scratchpad>/palette_vocab.rs`. It is reproduced verbatim in the appendix at
  the foot of this file, because a scratchpad does not survive the session.
- `builtin/mod.rs` re-exports no `PALETTE_*` names yet.

To resume: put the appendix back as `panel/palette.rs`, add `mod palette;`, add
it to `TABLES`, add the thirteen names to `builtin/mod.rs`'s re-export list —
**and convert the desktop palette in the same commit.**

## WHERE THIS STOPPED — resume here

**Working tree at compaction: `origin/main = 6b1567f6` plus UNCOMMITTED changes.
Nothing pushed. The changes are on disk and survive; this file says what they
are and what is missing.**

### Done, compiling, tests passing

**1. `builtin/panel.rs` split into `builtin/panel/`** — done with `git mv`, so
the history follows.

- `panel/mod.rs` (new) — the general argument for panel commands (moved up out
  of the old file's module doc), `mod` declarations, the re-export block,
  `TABLES`, `PANEL_COMMAND_COUNT` (a `const` loop over `TABLES`),
  `panel_commands()`, `panel_command_metas()`, `register_panel_commands()`.
- `panel/explorer.rs` — the explorer's modes, ids and table, now
  `pub(super) static EXPLORER`. Module doc rewritten to be about the explorer.
  344 lines.
- `panel/palette.rs` (new) — `PALETTE_MODE` and **12 verbs**, table
  `pub(super) static PALETTE`.
- `panel/tests.rs` (new) — the three tests that were in `panel.rs`, now written
  over `TABLES` so a panel added in a new file inherits them, **plus two new
  ones**: `every_way_of_reading_the_tables_agrees` and `no_panel_table_is_empty`.
- `builtin/mod.rs` — the `pub use panel::{...}` list gained the 13 `PALETTE_*`
  names.

⚠️ **`panel_command_metas()` changed shape**: it was
`const fn -> &'static [CommandMeta]`, it is now
`fn -> impl Iterator<Item = &'static CommandMeta>`. The tables are separate
statics, so there is no one slice to lend. **Two callers iterate and need
`.iter()` dropped**:

- `crates/iridium-panel/src/explorer/resolve.rs` → `panel_mode_of`
- `crates/iridium-panel/src/explorer/keymap_tests.rs` →
  `every_registered_panel_command_is_a_verb_the_panel_answers`

`cargo check -p iridium-editor --all-targets` exits 0 and the five panel tests
pass. **The rest of the workspace has NOT been checked since the signature
change** — those two callers are the expected breakage.

### ⛔ NOT DONE — and why the kernel half must not be committed alone

Twelve `palette.*` commands are now registered and **nothing resolves them**.
Committing that is exactly the #110 defect class: a vocabulary that validates in
`config.toml`, produces no diagnostic, and does nothing. #91 refused to commit
its own steps 1–2 for the same reason.

**So: finish the desktop palette conversion, or revert the palette half.**

---

## The desktop command palette — what is left

Ground, `apps/iridium-desktop/src/command_palette.rs`:

- `handle_key` at `:175`, the `match (chord(event.modifiers), event.key)` table
  at `:182-205`.
- The `Chord` enum at `:478` and `chord()` at `:490` — **delete both** when the
  last reader goes, as #91 did.

### The table as it stands, and the verb each row becomes

| chord | verb |
| --- | --- |
| `Escape`, `Ctrl+K`, `Meta+K` | `palette.dismiss` |
| `Enter` | `palette.accept` |
| `Up`, `Ctrl+P` | `palette.selectPrevious` |
| `Down`, `Ctrl+N` | `palette.selectNext` |
| `PageUp` | `palette.selectPageUp` |
| `PageDown` | `palette.selectPageDown` |
| `Left` / `Right` / `Home` / `End` | `palette.caret{Left,Right,Home,End}` |
| `Backspace` | `palette.queryBackspace` |
| `Delete` | `palette.queryDelete` |
| `Char(c)` | the typing fall-through — **not a verb** |
| anything else | swallowed |

### The five traps, carried from #91

1. **⛔ D-4, the invisible regression.** `chord()` reads only `ctrl`, `alt` and
   `meta` and **never looks at shift**, so `Shift+Down` works today. Every
   converted binding must spell shift `ModifierState::Any`. #91's mutation M2
   measured that **nothing else catches this** — 229 tests passed with the
   mutation in. The palette needs **its own** shift ratchet, over its own table
   and its own pattern constants.
2. **The fall-through must check modifiers before treating a `Char` as text.**
   `Ctrl+S` typed an `s` into the query in #91's first cut. Use the
   `types_text(modifiers)` shape.
3. **A command the panel merely *answers* rather than owns is not a `Verb`** and
   needs handling beside the enum. ⚠️ The palette looks clean here — `⌘K` while
   the palette is open is `palette.dismiss`, a verb it owns — but check for a
   method going dead, because the **compiler** is what caught this in #91, not a
   test.
4. **A mode-free user binding applies in every mode**, so `set_user_keymap` must
   filter the user layer to this panel's commands and scope each by its declared
   mode. `panel_mode_of` already does the lookup.
5. **A suppression carries no command**, so it cannot be scoped; pass it through
   mode-free and document it.

### Files to write, following the explorer exactly

- `command_palette/keymap.rs` — pattern constants (`PLAIN`, `CTRL`, `META`, all
  with shift `Any`), `default_keymap()`, `DEFAULT_BINDING_COUNT`, and
  `#[cfg(test)] EVERY_PATTERN` for the pattern-level ratchet.
- `command_palette/resolve.rs` — `Resolved` enum, `resolve_key`,
  `set_user_keymap`, `has_pending_keys`, `types_text`.
- `command_palette/verb.rs` — `Verb`, `Verb::ALL`, `const fn id()` returning the
  kernel constant so there is one spelling, `from_id()`.
- `command_palette.rs` is **811 lines** and over the 800 target already, so this
  is also the split it needed.

⚠️ **The desktop palette and the TUI palette are two implementations.** They
share the kernel's *names* and each owns its *keys*, which is the split #91
established. Do not try to share one `default_keymap()` across the faces.

### After it lands

The generated `[keys]` block picks the palette up for free — the desktop face
already contributes its layer through `FaceKeys` (#118), and the panel's
`default_keymap()` is reachable from the face. **Check whether
`crates/iridium-config/src/template_keys.rs` should also take the desktop
palette's layer**, or whether the face's single `FaceKeys` entry is enough.

---

## Order for the remaining six

Cheapest first, and each is independent:

1. **desktop command palette** (in progress)
2. **desktop history overlay** — 525 lines, a list and a jump, few verbs
3. **TUI command palette** — same vocabulary as 1, different keys
4. **TUI history panel** — same vocabulary as 2
5. **TUI search** — shares nothing; its own mode
6. **desktop context menu** — 707 lines, and #28's drawn overlay
7. **desktop menubar** — 426 lines, and ⛔ **#108 is still blocked on Tom
   clicking one menu item**. Do not touch the menubar's keys until he has.

## Not urgent

These are conventional keys — arrows, Enter, Escape — that nobody has asked to
retune. Tom's 8 Aug request named the oil case, which #91 delivered. The value
here is uniformity and the `[keys]` listing, not a complaint being answered.


---

# Appendix — ⛔ SUPERSEDED, kept only as the record

**This was the parked vocabulary. It is now real code** at
`crates/iridium-editor/src/commands/builtin/panel/palette.rs`, and the panel that
resolves it is at `apps/iridium-desktop/src/command_palette/`. Read the files,
not this. The table below is left standing because it is the record of what was
held back and why — nothing resolved it, which is the #110 defect class.

Twelve verbs under one mode `palette`:

| id | title |
| --- | --- |
| `palette.dismiss` | Close Command Palette |
| `palette.accept` | Run Selected Command |
| `palette.selectPrevious` | Select Previous Command |
| `palette.selectNext` | Select Next Command |
| `palette.selectPageUp` | Select A Page Up |
| `palette.selectPageDown` | Select A Page Down |
| `palette.caretLeft` | Move Caret Left In Query |
| `palette.caretRight` | Move Caret Right In Query |
| `palette.caretHome` | Move Caret To Query Start |
| `palette.caretEnd` | Move Caret To Query End |
| `palette.queryBackspace` | Delete In Query |
| `palette.queryDelete` | Delete Forward In Query |

Every one is `CommandMeta::scoped(..., CommandCategory::GENERAL, PALETTE_MODE)`.
`PALETTE_MODE` is `ModeName::from_static("palette")`.

⚠️ **`palette.open` is deliberately not among them.** Opening the palette is
something the editor can be asked from anywhere, so it stays the mode-free host
command in `builtin::host`. The same chord doing both — `⌘K` opens it and closes
it again — is *two* commands, which is exactly what the two modes make possible;
rebinding either leaves the other alone. This is the same shape as
`explorer.togglePanel` against the explorer's verbs, and it is the trap #91
called "a command the panel merely answers rather than owns" — except here the
close **is** a palette verb, so the palette owns both halves and the trap does
not apply. Confirm that by checking no method goes dead when the old table is
deleted; the compiler is what caught it last time.
