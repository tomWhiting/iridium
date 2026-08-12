# #91 — panel keys through `[keys]`, not a hard-coded table

13 Aug 2026, seat Doug. Design map with the rulings taken at this seat, because
both threads that could have carried a question to Tom (#108, #113) are already
waiting on him and this one does not need him.

## What Tom asked for

> "in terms of the oil case and keys generally, it would be good to have that
> keybindings configuration thing so we can set it to other things easily."
> — 8 Aug 2026

`~/.config/iridium/config.toml` has had a `[keys]` table since #59. It binds
**commands in the editor's keymap stack**. The explorer answers its own keys in
a `match` and never consults that stack, so no line a user can write moves any
of them.

## ⚠️ The task description was written before #112d and is stale

It names `apps/iridium-desktop/src/file_tree/`. That directory is now 67 lines
and contains **no key handling at all** — the panel moved into
`crates/iridium-panel/src/explorer/`, and the terminal face's
`crates/iridium-tui/src/frame/file_explorer/mod.rs:191` is a five-line wrapper
that delegates to it.

⭐ **That makes the job smaller than it reads.** The explorer's keys exist in
exactly one place and serve both faces, so converting them converts both.

## Ground, measured

Two tables, both in `crates/iridium-panel/src/explorer/`:

| file | lines | screen | bound arms | plus |
| --- | --- | --- | --- | --- |
| `keys.rs` | 508 | browse | 16 | a catch-all |
| `edit_keys.rs` | 421 | editing | 14 | a catch-all |
| `edit_keys.rs` | — | confirming | 2 | a catch-all |
| `edit_keys.rs` | — | refused | 1 | (an `if`, not a `match`) |

Counted from the arms, not estimated. Several arms bind two chords — `Up` and
`Ctrl+P` are one arm — so the number of *chords* to convert is larger than 33
and is not asserted here until the conversion table exists.

The other panels — `command_palette.rs`, `context_menu.rs`, `menubar.rs`,
`history_overlay.rs` in the desktop face, and `search/`, `history_panel/`,
`command_palette/` in the terminal face — each carry their own table too. **They
are not in this task**: the task body names the oil panel and the file tree and
nothing else, and their keys (arrows, `Enter`, `Escape`) are the conventional
ones nobody asks to retune. Recorded here so the count is known rather than
discovered: **seven further tables**, and the mechanism this task builds is what
they would each be converted onto.

## What already exists, and is the reason this is not a new mechanism

- `KeyBinding::in_mode` / `applies_in` — bindings are **already** mode-scoped,
  and a mode-scoped binding outranks a mode-free one for the same sequence.
- `Keymap::silence_typing_in` — a mode where an unclaimed printable key does
  **not** type itself. Written for exactly this shape.
- `KeymapStack::exact_match` / `has_continuation` — resolution and the
  multi-stroke question, both already taking `Option<&ModeName>`.
- `commands/builtin/host.rs` — commands the kernel *names* and does not
  implement, registered so they validate and bind. ⭐ **`explorer.togglePanel`
  and `explorer.toggleSidebar` are already two of them**, so the `explorer.*`
  namespace and its registration path are established, not invented here.

Nothing in this task adds a configuration mechanism. It adds *entries* to the
one that exists.

## The rulings

### D-1 — panel actions are host commands, registered in the kernel

⛔ **Not face-local ids.** `host.rs` already argues this and the argument holds
verbatim: three faces registering their own ids is how one action acquires three
names and a keymap stops being portable. The explorer is compiled into both
native faces from one source; its verbs must have one id each.

The practical half is that `[keys]` **validates**. `Workspace::push_keymap`
rejects a binding naming an unregistered command with `UnknownCommand`, so a
user writing `"ctrl+j" = "explorer.moveDown"` today gets a diagnostic. Once the
id is registered, the same line is accepted — and a *typo* still gets the
diagnostic, which is the half worth keeping.

### D-2 — the mode is derived from the id, not spelled in the file

A panel binding needs a mode; `[keys]` has no spelling for one. Three options
were priced:

| option | cost |
| --- | --- |
| **(a)** a `[keys.explorer]` sub-table | TOML requires sub-headers after all bare keys in `[keys]`; a user who writes them in the other order gets a parse error about something that is not their mistake |
| **(b)** a prefix in the value | a second grammar inside a string, and every diagnostic has to quote it back |
| **(c)** derive the mode from the command id | no config surface at all |

⭐ **(c).** The mode is not independent information — `explorer.moveDown` is
meaningless outside the explorer, and asking a user to say so twice is asking
them to keep two things in step for no gain. A binding whose command is a
registered explorer action is scoped to the explorer's mode; every other binding
is mode-free exactly as today.

**Revert cost if this is wrong: one function.** (a) can be added beside (c)
later without moving a line of the panel — the mode arrives at the same place
either way.

### D-3 — the panel resolves against its own stack, not the editor's

The purest shape is for the face to route every key through the editor's stack
while the panel is open, in mode `explorer`, with `silence_typing_in` handling
the filter. One resolver, one path.

⛔ **Not yet, and deliberately.** It moves the panel-open ↔ mode-set lifecycle
into *both* faces' input loops, across four screen transitions (browse → edit →
confirm → refused) that today are the panel's private business. That is a change
to the thing every keystroke goes through, made in a window where nobody can
click the result.

The panel holds its own `KeymapStack` built from the same `Keymap` type, fed the
same `[keys]` layer the editor gets. **One table, one type, one config file, two
resolver instances** — which is a second *site*, not a second mechanism, and
this is the distinction the task's "not a second configuration mechanism beside
it" is actually about.

⚠️ It does not foreclose the pure shape: everything D-1 and D-2 build is what
that shape needs too. Named as a known gap, priced above.

### D-4 — Shift must be `Any` on every converted binding, or this is a regression

⛔ **The trap, and it is invisible.** `chord()` (`panel.rs:564`) reads only
`ctrl`, `alt` and `meta` — **shift is not in the match at all**. So today
`Shift+Down` moves the selection, `Ctrl+Shift+D` strikes a row, and
`Ctrl+Shift+N` moves down, because all three collapse onto a chord that never
asked about shift.

A `KeyPress` carries shift, and a binding that does not spell it `Any` requires
it absent. A faithful conversion therefore spells shift `Any` on **every**
converted binding. A conversion that forgets compiles, passes every test written
against the unshifted spelling, and silently drops the chords that work today.

**Gated, not remembered:** a test presses every converted binding a second time
with shift held and asserts the same outcome.

### D-5 — the query and the name keep their catch-all, below the keymap

`(Plain, Char(c))` is the last arm of both tables and it is not a binding — it
is "every printable character that nothing else claimed goes into the field on
this screen", which is the filter while browsing and the filename while editing.

It stays a catch-all *below* resolution rather than becoming 26 bindings. A user
who binds `explorer.strikeRow` to a bare letter takes that letter out of the
filter, which is the honest consequence of what they asked for and is why
resolution runs first.

### D-6 — a user may unbind `Escape`, and the panel must still be closable

`"escape" = ""` is a legal line. Bound to nothing in the explorer's mode, the
panel loses its way out — but **not** its only one: `explorer.togglePanel` is a
host command the *editor* stack owns, and the face answers it whatever the panel
thinks. So the panel is always closable from outside, and the panel is not given
a hard-coded `Escape` that a user cannot move. Stated because the alternative —
an unmovable key — is the thing this task exists to remove.

## The build

1. `explorer/action.rs` — `ExplorerAction`, one variant per verb, with `id()`
   and `from_id()`. The single spelling of every panel command id.
2. `commands/builtin/panel.rs` — the metas, registered beside the host table so
   `[keys]` validates them. Count assertions updated where they are asserted.
3. `explorer/default_keys.rs` — the default `Keymap`, four modes, **shift `Any`
   throughout** (D-4).
4. `explorer/resolve.rs` — the stack, the pending-sequence state (a user may
   bind `ctrl+k ctrl+d`, and a binding that silently never fires is the defect
   class this repo does not ship), and `set_user_keymap`.
5. `keys.rs` / `edit_keys.rs` — the `match (chord, key)` becomes
   `match action`, and the two module doc tables are rewritten to name the
   command ids rather than the chords, since the chords are now defaults.
6. Both faces pass the config layer to the panel.
7. Tests: the shift ratchet (D-4), a `[keys]` line moving a real key end to end,
   an unbind, a multi-stroke binding, and the catch-all still reaching the
   filter.

## What is proven, and what is not

Nothing yet — this document is the map. The proof obligations are listed in step
7 and every one of them is a `cargo test`, because the explorer's key handling
has no GPU in it and never did.

⚠️ **Not provable here:** that the defaults still *feel* right in the hand. The
conversion is meant to change nothing about what any key does today, and D-4 is
the one place where a mistake would be silent rather than red.
