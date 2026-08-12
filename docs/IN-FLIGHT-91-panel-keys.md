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

### D-2 — the mode is a property of the *command*, declared on its metadata

A panel binding needs a mode; `[keys]` has no spelling for one. Three options
were priced:

| option | cost |
| --- | --- |
| **(a)** a `[keys.explorer]` sub-table | TOML requires sub-headers after all bare keys in `[keys]`; a user who writes them in the other order gets a parse error about something that is not their mistake |
| **(b)** a prefix in the value | a second grammar inside a string, and every diagnostic has to quote it back |
| **(c)** infer the mode from the command id's prefix | a string convention doing load-bearing work, invisible to the type system |
| **(d)** `CommandMeta` gains `mode: Option<ModeName>` | one field on a struct that is already the answer to "what is this command" |

⭐ **(d), and it was reached by finding what (c) could not answer.**

`CommandMeta` has no visibility flag, and every registered command is a palette
entry. So registering 26 explorer verbs puts *Move Down* and *Backspace In Row*
in the command palette, where they are meaningless — the panel they belong to is
shut, and running one from the palette is not a thing that can happen.

One field answers both questions at once. A command that names a mode is scoped
to that mode when a `[keys]` line binds it, **and** is not a global palette
entry, because those are the same fact stated once: this command belongs to a
context rather than to the editor. `ModeName::from_static` is `const`, so the
static tables carry it with no allocation and no second table to keep in step.

⛔ **Not the id prefix.** `explorer.togglePanel` is a genuine global — it opens
the panel from outside it, and it is already a host command with a palette entry
and aliases. A prefix rule would scope it into the mode it exists to enter, and
the panel would become unopenable from the palette. The prefix and the mode
genuinely disagree for that id, which is the case that kills (c).

**Revert cost: one field, and the palette filter that reads it.**

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

## What was built, and what changed on the way

Steps 1–7 all landed. Two things the map did not foresee, both found by building:

### ⭐ `CommandMeta` gained the mode, and it answered a second question

The map's D-2 first chose "infer the mode from the id prefix". Writing the table
showed why that cannot work: **every registered command is a palette entry**, so
28 explorer verbs would have put *Move Down* and *Delete In Row* in the command
palette, where they act on a screen that is not open.

One field answers both. `CommandMeta::scoped` names the mode; a `[keys]` line
binding that command is scoped to it, **and** `palette_order` and
`palette::search` leave it out. And the prefix rule is refuted outright by
`explorer.togglePanel`, which shares the prefix, is a genuine global, and must
keep its palette entry — it is how the panel is opened.

### ⛔ Passing the user's whole layer to the panel would have broken `Tab`

A mode-free binding applies in **every** mode. So a user who bound `Tab` to
`edit.indent` for the *document* would have found `Tab` had stopped opening the
oil buffer, having never mentioned the explorer. `set_user_keymap` therefore
keeps only bindings naming a panel command and scopes each by the mode its
command declares — which is D-2 doing the real work rather than being a tidy
idea. Suppressions carry no command and so cannot be scoped; they pass through
mode-free, and the panel stays closable because the toggle is the editor's.

## Two regressions caught while converting, both by things already in the repo

1. **`⌘⌥E` stopped closing the panel from inside an editing session.** The
   toggle is not a [`Verb`], so it fell through the new dispatch. Caught by the
   *compiler* — `close_from_edit` went dead — not by a test.
2. **`Ctrl+S` typed an `s` into the query.** The new fall-through read the
   character without checking modifiers; the old table only fed `Chord::Plain`
   characters to the field. Caught by
   `a_key_the_panel_does_not_bind_is_swallowed_rather_than_typed`, which existed
   already.

> **📌 The law: a conversion is exactly as safe as the suite that existed before
> it.** Both defects were mine, both were introduced in a refactor whose whole
> claim was that nothing would change, and neither was found by anything written
> *for* the conversion.

## Mutations, measured

| # | mutation | result |
| --- | --- | --- |
| M1 | `set_user_keymap` accepts the user's layer unfiltered | 1 fails — `an_editor_binding_in_the_users_layer_does_not_shadow_a_panel_key` |
| M2 | `PLAIN` forbids shift instead of ignoring it | 2 fail — both shift ratchets |

⭐ **M2 is the interesting one.** *Only* the two ratchets fired; all 229 other
tests passed with the mutation in place. That is D-4's claim measured rather than
asserted: a conversion that forgot shift would have shipped, and no behavioural
test in this repository would have said a word.

## ⚠️ The terminal face is not wired, because there is nothing to wire

Measured, not assumed: `apps/iridium/src` references `iridium_config` exactly
twice, both for `iridium_config::theme`. **The terminal face does not read
`[keys]` at all.** Its explorer therefore runs on the defaults, which is correct
and complete for that face today.

No passthrough was added to `FileExplorerPanel` for it. A method no caller calls
is the parsed-and-never-read defect #110 was about, and adding one here would
have been that defect wearing this task's clothes. When that face gains
configuration reading it calls the same `set_user_keymap` the desktop does.

## What is proven, and what is not

**Proven by `cargo test`, with all ten gates green:** a `[keys]` line moves a
real panel key; the keys it did not name still work; an editor binding does not
shadow a panel one; a multi-stroke binding fires and its leader is held; an
abandoned chord does not become a filter; an unbind stops a key working; every
verb has a default; every default ignores shift; every registered panel command
is answered; a panel command validates against the editor's registry **and** a
misspelling of one still does not.

The 215 tests that existed before the conversion pass unchanged, which is the
strongest single statement available that the chords still do what they did.

⚠️ **Not proven:** that any of it is right *in the hand*. Nothing was clicked —
no face was run. The conversion is meant to change nothing about what any key
does, and M2 shows exactly where a mistake would have been silent rather than
red.

⚠️ **Also not proven:** that a binding the *editor* refused (cross-layer
shadowing) is refused by the panel. The panel receives the bindings as the file
wrote them, and the editor's refusals are about the editor's stack. It costs
nothing today — a panel command cannot shadow an editor chord — and it is where
to look first if the two ever disagree about a sequence.
