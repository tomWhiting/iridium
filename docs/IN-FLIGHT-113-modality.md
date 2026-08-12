# #113 — the terminal face, modal by default

**Status: design map, ground verified, nothing built.** Written 13 Aug 2026.

The task is one line — *terminal face: modal by default, non-modal keymap
available* — and the handoff has carried one instruction about it for three
sessions: **the pending-operator mechanism before any modal keymap is
authored.** That instruction turns out to be aimed at the wrong gap. The
mechanism that blocks modality is not operators; it is that **typing is not a
binding**, so a normal mode cannot switch typing off by binding keys.

Everything below was read out of the code, not inferred.

---

## Ground

### G-1 — The mode machinery is complete, and nothing uses it

The resolver has held modes since it was written:

| | |
| --- | --- |
| `KeymapResolver.mode: Option<ModeName>` | `commands/resolver.rs:164` |
| `KeyBinding::in_mode` — scope a binding to a mode | `commands/binding.rs:105` |
| `KeyBinding::enter_mode` — a stroke that only switches mode | `commands/binding.rs:89` |
| `KeyBinding::then_enter_mode` — run *and* switch | `commands/binding.rs:116` |
| `Resolution::ModeEntered(ModeName)` | `commands/resolver.rs:25` |
| `set_mode` discards the pending buffer, for a stated reason | `commands/resolver.rs:228` |
| `Editor::mode` / `Editor::set_mode` | `editor/core.rs:755`, `:765` |

⭐ **The terminal face already draws it.** `frame/status.rs:76` renders
`editor.mode()` whenever there is one, with a module doc saying *"a modal keymap
that does not say which mode it is in is unusable"*; `app/view.rs:220` already
renders the pending sequence. `ModeName` appears in 20 files — the machinery,
its tests, and the statusline that displays it. **Nothing sets a mode outside
tests.**

So the display half of modality is done and has been for a while. What is
missing is a keymap that uses it, and one mechanism it cannot work without.

### G-2 — Counts resolve, and then die

`with_count_prefix` appears **zero times** in `commands/default_keymap.rs`, so
no digit is ever absorbed into a count today. `CommandArgs::repeat_count()`
(`commands/args.rs:70`) is documented as *"the convention every repeat-capable
command wants: `dw` deletes one word, `3dw` deletes three"* — and exactly two
dispatch arms read a count at all: `Action::HistoryRedoBranch`
(`actions/run.rs:200`) and goto-line (`edits.rs:325`).

⚠️ **No motion and no edit repeats.** `3j` is not a feature that is switched
off; it is a feature that has never been wired. Every modal grammar needs it.

### G-3 — Motions are already the right shape

`input/keyboard/motions.rs` is pure: `word_left`, `word_right`,
`line_start_smart`, `line_end`, `document_end`, `vertical_move_by` — all
`(&Document, Position) -> Position`. `apply_to_all(cursor, extend, f)` is what
turns one into either a caret move or a selection extension, and **every
navigation action in the table already has a `…Select` twin**
(`actions/table.rs`). That pairing is what makes a selection-first grammar cost
almost nothing (see D-1).

### G-4 — There is no operator-pending mechanism

The string "operator" appears nowhere in `input/keyboard/`. Confirmed, and it
is the thing the handoff said to build first. See D-1 for why it may never need
building.

### G-5 — ⚠️ THE BLOCKER: typing is not a binding

`EDIT_INSERT_CHARACTER` is a registered command bound to **nothing** — zero
occurrences in `default_keymap.rs`, and the default-keymap test is literally
named `every_registered_command_is_bound_except_the_typing_fall_through`.
Self-insert happens in `dispatch::fall_through`, reached only on
`Resolution::NoMatch` (`input/keyboard/dispatch.rs:116`) — that is, **after the
keymap has already declined the key.**

The consequence, stated plainly: **in a normal mode, every printable character
that is not bound to something would be inserted into the document.** `q`, `z`,
`;` — all typed straight into the file.

And it cannot be papered over with suppression bindings. `KeyBinding::unbound`
exists, but resolution step 5 says a suppression means *the sequence is
unbound and resolution continues at step 6* (`resolver.rs:303`) — which ends at
`NoMatch`, which self-inserts. **Binding every printable key to nothing in
normal mode would still type them.** That is a measured elimination, not a
guess.

### G-6 — The config keymap has no modes and no selector

`[keys]` is a flat chord → command map (`docs/CONFIG.md:115`).
`crates/iridium-config/src/keys.rs` knows `accepts_count` but nothing about
modes. There is also **no setting that chooses a keymap**, so "non-modal keymap
available" has nowhere to be expressed today.

---

## Rulings

### D-1 — The Vim/Helix fork is deferred on purpose, not dodged

Two grammars are on the table, and they price very differently against this
kernel:

| | **Vim** — operator × motion | **Helix** — selection first |
| --- | --- | --- |
| `dw` | `d` waits, `w` supplies a range | `w` selects the word, `d` deletes the selection |
| new kernel mechanism | pending-operator state, a motion classification the kernel can consult, charwise/linewise | **none** — every `…Select` twin already exists (G-3) |
| multi-cursor | operators must fan out per cursor | falls out: `apply_to_all` already extends every selection |
| the keymap is | mechanism *and* data | **data only** |

**My lean is Helix/selection-first**, for one reason that is about this codebase
rather than about taste: it makes the modal keymap *data over commands that
already exist*, which means it can live in the config format (D-3) and be
rebound, rather than being a second dispatch path welded into the kernel. This
kernel's multi-cursor machinery and its `expand`/`shrink` syntax verb are
already selection-first; a Vim grammar would be the only part of it that isn't.

**But it is not my call, because it decides which muscle memory works**, and I
have no evidence about which one Tom's hands have. So the fork is **put to him**
— and in the meantime **steps 1–3 below are needed by both answers**, so no work
waits on the reply and no work is wasted by it.

⚠️ **Revert cost if he picks Vim after the ground lands: the keymap file and the
operator mechanism (G-4).** The three steps below are untouched by the choice.
Revert cost if the *ground* is built for one grammar and he picks the other:
much larger, which is exactly why it is being built grammar-agnostic.

### D-2 — Normal mode stops typing because the *keymap* says so, not the kernel

Three ways to close G-5, and two of them are wrong:

- ~~Bind every printable key to nothing per mode~~ — **impossible**, measured in
  G-5: suppression falls through to self-insert.
- ~~Teach `dispatch::fall_through` about modes~~ — the kernel would grow a
  policy about which mode names type, breaking the promise written at
  `binding.rs:23`: *a modal keymap is expressible **without the kernel knowing
  any mode**.*
- ✅ **A keymap declares, per mode, whether an unclaimed printable key
  self-inserts.** `Keymap` gains it, `KeymapStack` answers for the active mode,
  and `dispatch` consults that one bool before calling `fall_through`.

The kernel still knows no mode names. It asks the keymap a question and the
keymap answers, which is the same shape as every other thing modes do here.
Default is `true`, so **the non-modal keymap and both existing faces are
unchanged by construction** — a keymap that never mentions a mode can never
turn typing off.

### D-3 — Modes reach the config format as TOML sections

`[keys]` keeps meaning *every mode*. A mode-scoped section is
`[keys.normal]`. A binding that switches mode is an inline table, because a
bare string has nowhere to put the second fact:

```toml
[keys.normal]
"i" = { mode = "insert" }                                 # switch only
"d" = { command = "edit.deleteSelection", mode = "insert" }  # run, then switch
"w" = "cursor.wordRightSelect"                            # plain command
```

Chosen over a `"mode:insert"` string sigil because the sigil is unparseable the
moment a command id could contain a colon, and over a separate `[modes]` table
because that would put a binding's mode in a different place from the binding.

### D-4 — A count repeats *once*, in one history command

`3dd` must be **one** undo step, not three. So the count cannot be "run the
action N times" at the dispatch layer — that produces N commands and N history
entries. The two candidate shapes, to be measured against that test when step 3
lands:

- the count reaches `apply_to_all`, and motions compose N times before any
  cursor state is written;
- the count reaches the *verb*, and line/edit verbs build one compound.

Whichever wins, the guarding test is written first and named for the
requirement: **`three_of_a_verb_is_one_undo_step`**.

### D-5 — The default flips last, behind a setting

`[editor] keymap = "modal" | "standard"`. The terminal face defaults to
`modal`; the desktop face defaults to `standard` and is not part of this task.

⚠️ **The flip is the final commit, not the first.** Every step before it lands
with the terminal face behaving exactly as it does today, so a half-built modal
grammar can never be the thing Tom's editor comes up in.

### D-6 — Panels are outside the mode system

The explorer, the palette, the search overlay and the prompt intercept keys
before the editor sees them (`app/mod.rs:342` onward). The mode is the
**document's**, not the application's: opening a panel does not change it, and
closing one returns to whatever it was. A panel that wanted its own mode would
be inventing a second mode system, and the one we have belongs to the resolver
that panels do not use.

### D-7 — The terminal `file.saveAs` key, carried from #111

#111 left this here deliberately: that face's `CTRL` pattern ignores `Shift`
because a terminal cannot always report it, so the `Ctrl+S` / `Ctrl+⇧S` split
the desktop uses is unavailable. It is decided **in step 4**, with the rest of
the keymap, because a modal grammar has a natural place to put it and a
non-modal one does not — and choosing now would be choosing before the grammar
is known.

---

## Build order

Steps 1–3 are needed whichever grammar wins D-1.

1. **The typing gate (G-5, D-2).** `Keymap` learns whether a mode self-inserts;
   `dispatch` asks. Test: a hand-built two-mode keymap where `q` types in one
   mode and does nothing in the other. **Nothing about the shipped faces
   changes** — proven by the existing suite staying green with no edits.
2. **A minimal modal keymap, built in, off by default.** Normal and insert only:
   the motions that already exist, `i`/`a` to insert, `Escape` to normal. This is
   the first point where the whole path — resolver, mode, statusline, typing
   gate — is exercised end to end, and it is small enough to be a probe rather
   than a commitment.
3. **Counts (G-2, D-4).** `with_count_prefix` on the motion bindings, the count
   consumed, `three_of_a_verb_is_one_undo_step` written first.
4. **The grammar (D-1), once ruled.** The full keymap, and D-7 with it.
5. **The config format (G-6, D-3)** and **the setting (D-5)**, and the default
   flips in that last commit.

Steps 1–3 are pure `cargo test` — the modal path is proven correct before it is
ever the thing that comes up on screen.

---

## ✅ Step 1 LANDED — the typing gate

`Keymap::silence_typing_in(mode)` and `Keymap::types_unclaimed_keys(mode)`;
`KeymapStack::types_unclaimed_keys` over them; one gate inside
`dispatch::fall_through`.

**Four decisions inside the mechanism, each written where it lives:**

1. **Any layer that silences a mode silences the stack** — `all`, not the
   highest layer winning as it does for bindings. Two layers disagreeing about
   whether a mode types is not a precedence question, and of the two readings
   only one is safe: *a key that does nothing is a key the user presses again,
   while a key that types is text in a document nobody asked to change.*
2. **`None` can never be silenced.** `silent_modes` holds `ModeName`s, so a
   keymap with no active mode always types — the invariant that makes it
   impossible for a non-modal keymap to stop typing by accident.
3. **The gate sits *inside* the character branch, not above it.** What the
   keymap declared is that the mode does not **type**; a key carrying no
   character was never the gate's to swallow, and a host watching for unhandled
   keys must still see it.
4. **A swallowed character is `Handled`, not `Ignored`.** A host that saw
   `Ignored` would be entitled to type the key itself — the exact outcome the
   mode was declared to prevent.

**Eleven tests.** Five in `dispatch_tests.rs` (silenced mode does not type; the
same key types the moment the mode is left, including with the silencing layer
still on the stack; a key with no character stays the host's; a *bound* key
still runs in a silenced mode; and `no_shipped_keymap_silences_anything`), four
in `layer_tests.rs`, two in `serde_tests.rs`.

⭐ **Both halves proven load-bearing by mutation, not asserted:**

| sabotage | what failed |
| --- | --- |
| the gate deleted from `fall_through` | `a_silenced_mode_does_not_type_a_key_no_binding_claimed`, `the_same_key_types_the_moment_the_mode_is_left` — and nothing else |
| the stack's `all` changed to `any` | those two **plus** `one_layer_silencing_a_mode_silences_the_whole_stack` |

**Nothing about either shipped face changed**, and that is proven rather than
claimed: the 1,319 existing kernel tests pass with **no existing test edited**,
and `no_shipped_keymap_silences_anything` walks every layer of the real handler
and asserts none of them has opted in. The gate is opt-in and nothing has opted
in yet.

---

## ✅ Step 2 LANDED — the minimal modal keymap

`commands/modal_keymap.rs`: two modes, **seven bindings**, `hjkl` + `i` + `a` +
`Escape`. **Nothing installs it.** No face pushes it and the setting that would
choose one does not exist yet — D-5 says the default flips last.

⭐ **A keymap now declares the mode a session begins in**, alongside the modes it
silences. `Keymap::set_initial_mode` / `initial_mode`, and
`KeymapStack::initial_mode` over them. This was not in the map and is the one
thing step 2 added to the mechanism: a keymap that silences typing in `normal`
but does not say a session *starts* in `normal` is only half a keymap — it works
if whoever installs it happens to remember the name, and fails **silently** if
they do not, as a document that types normally and answers to no motion.

⚠️ **The stack resolves the two declarations by opposite rules, deliberately.**
Silencing is `all` — any layer that silences a mode silences it. The initial
mode is the **highest** layer that names one. A safety property and a single
value are not the same kind of fact, and each is written where it is decided.

**Eleven tests**, all driving the real `KeyboardHandler` against a real
`Document` rather than asserting the table was typed out correctly. Every
binding is exercised, plus `every_binding_in_the_modal_keymap_is_reachable`
(the hint index's own oracle, applied to catch a mode-scoped binding a mode-free
one in the default layer would outrank) and `the_default_keymap_alone_names_no_mode`.

⭐ **Both declarations proven load-bearing by mutation:**

| sabotage | what failed |
| --- | --- |
| `silence_typing_in(NORMAL)` removed | 3 tests — the swallowed letter, and both round trips through insert |
| `set_initial_mode(NORMAL)` removed | **6** — including every motion test |

The second is the failure the doc comment predicts: with no initial mode the
session is in no mode, so no mode-scoped binding matches and typing is not
silenced. The keymap is fully installed and completely inert.

**Two of my own test bugs, worth recording because both would have passed for
the wrong reason in a different shape:**

1. Reachability is settled by **pointer identity**, so pushing `modal.clone()`
   and then iterating the original's bindings reported every one unreachable.
   The bindings must be read back out of the stack.
2. A keymap validated against `builtin_registry()` rather than
   `default_registry()` fails on `history.togglePanel` — the default keymap
   binds host commands the built-in table alone does not know. That is already
   written down as a statement rather than a quirk, in
   `the_default_keymap_does_not_validate_against_the_builtins_alone`.

---

## ✅ Step 3 LANDED — counts

`3l`, `3j`, `3w` work. `with_count_prefix()` on the modal keymap's four motion
bindings, and `ctx.args.repeat_count()` consumed in `run_action`.

⚠️ **Not every motion is countable, and the split is measured rather than
tasteful.** `line_start_smart` **toggles** between the first non-whitespace
column and column zero (`motions.rs:198`), so applying it twice returns the
caret to where it started; the document and line boundaries are absolute, so
applying them twice is the same as once. Repeating either would be wrong in one
case and pointless in the other. The two helpers now say which is which by
name:

| helper | motions | how the count applies |
| --- | --- | --- |
| `apply_motion` | line start/end, document start/end | **not at all** — once, and only once |
| `apply_repeated_motion` | char and word, both directions | composed `count` times |
| `vertical_motion` / `page_motion` | line and page, both directions | a **hop size**, not a repetition — `vertical_move_by` already took one and clamps |

**D-4 satisfied, and named for it.** `three_of_a_verb_is_one_undo_step` presses
`3` then `l`, asserts exactly **one** `Command` comes back, applies it, then
applies its inverse and asserts the caret is back at column 0 — not two thirds
of the way along. That is the whole reason the count is not "run the action
three times" at the dispatch layer.

⭐ **The early break, and a false claim of my own caught by measuring it.**
`apply_repeated_motion` stops as soon as the motion returns the position it was
given, which bounds `999999999l` by the document rather than by the number
typed — no arbitrary cap nobody could predict from outside. I wrote in the test
comment that without the break the test "would never finish."

**That was wrong, and the sabotage run proved it.** With the break removed the
test *passes*, in **174 seconds** instead of microseconds. It guards the
clamping and nothing else — *a test that can only pass proves nothing about the
thing it claims to guard.* A wall-clock assertion would close that gap and open
a worse one, so the real guard counts calls:
`a_repeated_motion_stops_calling_the_motion_once_it_stops_moving` asserts
exactly **4** invocations on a three-character line, and fails at 1,000,000
in 0.28s without the break.

**Nothing about the shipped faces changes**, by construction rather than by
care: `repeat_count()` is 1 unless a binding declared `with_count_prefix`, and
no binding in the non-modal default declares one — a count is only ever
reachable in a mode where digits are not text, which
`a_digit_is_text_in_insert_mode` pins from the other side.

⭐ **Both halves proven load-bearing by mutation:**

| sabotage | what failed |
| --- | --- |
| the repeat loop ignores the count | 4 tests |
| the bindings stop declaring `with_count_prefix` | 5 — the four above plus the vertical hop, since no digit is absorbed at all |
| the early break removed | the call-counting guard, at 1,000,000 vs 4 |

---

## What #113 does not do

- **The desktop face.** D-5. It defaults to `standard` and this task does not
  touch its keymap.
- **A Vim-compatibility promise.** Neither grammar is a bug-for-bug port, and
  the map does not pretend otherwise. Matching an edge case corpus is not a
  goal; being coherent is.
- **Macros, registers, marks, `:` ex commands.** None are prerequisites for
  modality and each is its own task if it is ever wanted.
- **Panel modality.** D-6.
