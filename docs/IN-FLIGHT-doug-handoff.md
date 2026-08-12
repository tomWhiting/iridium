# Doug — in-flight state, 12 Aug 2026

Written because a `/compact` keeps arriving and everything not in a file is
lost when one does. Rewritten after the install landed; the previous head
described an action that has since finished.

## Closed since the last rewrite

- **The desktop install** — done, and verified by the receipt rather than the
  exit code: bundle date 9 Aug 11:55 → **12 Aug 16:17**, and `strings` on the
  binary went `file.open` 0 → 1, `project.open` 0 → 2, `project.set` 0 → 1.
  ⭐ The grep matters more than the date: a timestamp says a file was written,
  only the string says *this* change is in it.
- `aa66c00c` #69 fixed · `8b1b350d` extension rulings + receipt ·
  `2882ce58` liminal's ground, two riders, one corrected claim.
- **Verified from the remote**: `origin/main = 2882ce58`,
  `git rev-list --count origin/main..HEAD` = 0, measured at fetch.

## #111 — New File and the Save As way in (this lane)

**What was actually wrong.** `save_as` was already written, tested and
complete. `⌘S` reaches it only for an **unnamed** buffer, because for a named
one it must write rather than ask — so the whole verb was reachable from one
state and invisible from every other. The fix is a door, not a feature.
`file.new` was genuinely absent: an untitled buffer appeared only when the
last tab closed.

**Shipped in `apps/iridium-desktop`:**

| | |
| --- | --- |
| `file.saveAs` | `⌘⇧S` / `Ctrl+⇧S`, prompt **prefilled** with the current path, caret at the end |
| `file.new` | `⌘N` / `Ctrl+N`, additive tab in the active tab's group, asks nothing |
| overwrite | a file already at the target is **asked about**, not refused and not silently replaced — `Deed::OverwriteWith(PathBuf)` |
| relative names | resolved against the **session root**, not the process cwd |
| byte-order mark | survives a save-as, because the write clones and `rename`s rather than building a fresh `TextFile` |
| a failed save-as | leaves the document attached to the file it had — the reason the write goes through a clone |
| read-only | protects the file the buffer came from, not the buffer's text: writing back is refused, writing elsewhere is not |

⚠️ **Two behaviour changes worth knowing about.** `⌘⇧S` and `Ctrl+⇧S` used to
*save* — not by anyone's decision, but because the save rows spelled `Shift`
`Any` and nothing else claimed the shifted chord. And saving to the path the
document already has now **delegates to `save`**, so it gets the byte-comparison
staleness guard instead of a nuisance "this file exists" question.

**Both mechanism claims were proven by mutation, not asserted:**

1. Reverting the clone+`rename` to `TextFile::new` → exactly one test failed,
   `a_byte_order_mark_survives_a_save_under_a_new_name`. Nothing else is
   accidentally coupled to it.
2. Reverting the save rows to `Shift`-ignoring while keeping the save-as rows
   → **all 508 still passed**. So the forbid is *explicitness, not necessity*:
   a `Required` `Shift` does outrank an `Any` one inside a layer. That claim is
   written in the code and is now measured on this pair, not inherited from the
   `⌘O` / `⌘⇧O` pair it was first measured on.

**Deliberately not done, and why** — `file.saveAs` and `file.new` are
desktop-only, stated in `commands/mod.rs` rather than left to be noticed:

- The terminal face **has** `App::save_as`, written and tested, with the same
  missing door. What does not transfer is the **key**: that face's own `CTRL`
  pattern ignores `Shift` for a stated reason — *a terminal cannot always
  report it* — so the `Ctrl+S` / `Ctrl+⇧S` split is unavailable there. What
  replaces it (another chord, or a palette-only row like `commands.list`) is a
  ruling about that face and belongs with **#113**, not transcribed from here.
- `file.new` is a *different verb* on a face with one buffer: it would have to
  discard the document and need the confirmation a reload already carries.
  Same word, different stakes — which is the drift the one-id rule warns about.

## Owed to people

- **Hermes Crumpet** (`dm:5b70322e-e7a9-451c-91ca-a3dfa7b05bd9`) — ✅ **PAID
  13 Aug 2026.** He corrected our doc (resume is record-level; a shed
  subscription feed is terminal), refused to invent a latency number, and
  answered the surface-vocabulary question that unblocked the irreversible
  decision.

  ⭐ **How it got paid is the part worth keeping.** The debt outlived three
  compactions with a note saying its content "did not survive — recover it from
  the session transcript". That note was wrong and cost real delay: the
  substance was in `docs/EXTENSIBILITY.md` §0f and §"honest comparison" the
  whole time, because it had been written down *when it was used* rather than
  when it was received. **Check what the work already records before opening a
  transcript.** A debt that feels unrecoverable is often only unindexed.
- **Vesper Lynd** (`dm:5849e0d8-4802-4869-8e0f-9f7fd187e198`) — answered on the
  fleet workflow, then **corrected herself against her own answer**, and a
  reply is owed. Her findings, kept here because they change what I build:
  - ⚠️ **The correction, and my own error inside it.** Her first answer said
    the fleet has no isolation mechanism, measured across *two* named
    documents. There are **three**. She had read `norn_fleet.awl`, the
    judging-only one, and answered as though it were "the fleet"; she had
    never opened `mm_fleet_loop.awl`, which is the one that has actually been
    run. **My part**: her original message named the two files it searched, and
    I relayed it to Tom as "both fleet documents" — turning a bounded
    measurement into a claim about the whole class. *An absence claim carries
    its search scope, and a relay that drops the scope makes a stronger claim
    than the evidence supports.*
  - **`mm_fleet_loop` is a sequential single-writer loop** — plan, build,
    gate, review, decide, `until not decision == "CONTINUE"`, bounded by an
    operator-named `max_passes` with no default. One builder at a time, so it
    needs no worktree isolation: it never has two writers. Its gate legs are
    declared `run` calls returning `{exit_code, stdout, stderr}`, and its own
    comment states the rule independently — *"an unmeasured gate is never
    summed into green or red."* That meets requirement 2 as built and makes
    requirement 1 moot by design.
  - What survives, narrowed: **`mm_fleet_swarm.awl` does fan out concurrent
    writers** (`distribute lane in lanes`) with no isolation — measured, zero
    hits. Real gap, but not the document I would use.
  - **AWL's declared `run` is the receipt mechanism I want**: the command's own
    outcome record (`exit_code`/`stdout`/`stderr`) produced by the dispatcher,
    with the program written out in the document rather than named by a
    parameter, so an agent cannot forge or summarise it. Non-zero exit is
    retryable *carrying its own output*; unparseable `run json` is terminal.
    `norn_fleet` does not use it — it uses the `agent` seam, which returns the
    agent's own account — but `mm_fleet_loop` **does**, which is the whole
    substance of her correction. Her revised offer is not "write a document"
    but "point the existing one at your repo and your ten-gate script".
  - **Strict collection**: verdicts pair with inputs *by position*, so a unit
    that fails after its retries fails the whole run rather than returning a
    gap dressed as a result. All-or-nothing, but finished units are recorded
    events, so a late failure loses the report, not the evidence.
  - **Do not plan on resume**: the replay contract means completed steps are
    *returned* rather than re-executed, but the fleet-run resume verb is filed
    and **not built** (their #205), and there is a live P0 (#82) where engine
    state went missing mid-run and killed 55 minutes of real work.
  - Two habits portable to my own subagents today with no instrument change:
    the **control step** (ask a question whose answer you already know before
    any that matter, and refuse the whole run on a wrong reply — a broken
    pipeline and a working one both return well-formed strings) and **strict
    collection** (no result may be absent, because a gap and a pass look
    identical downstream).

## #112 — the oil surface (IN PROGRESS)

**Part A, taller: DONE.** The explorer shared `PANEL_MAX_VISIBLE_ROWS = 12`
with the command palette and the undo tree. Twelve is right for a panel that
is *queried* — you type three characters and take the top row — and wrong for
one that is *browsed*. It now has `EXPLORER_MAX_VISIBLE_ROWS = 30`, still
clamped by `fit.max_interior_rows - 1`.

⭐ **The old cap was leaving most of the window unused, measured not assumed.**
`fit_for` gives a panel `1 - TOP_ANCHOR_FRACTION` (88%) of the window height
less padding, and a row costs `font_size × line_height` = `14 × 1.4` logical
px. A 1440×900 laptop affords **39** interior rows; the panel drew twelve.
Thirty rather than "as many as fit" because the window clamp is what protects
a short window, so the constant is only an upper *taste* bound — and the
full-height column Tom also asked for is the **sidebar**, a placement, not a
bigger number.

Proven by mutation: reverting to the shared constant makes the new test report
**13 rows into a window that affords 26**. There is a paired short-window test,
because raising a ceiling is only safe if the clamp under it still binds — a
tall-case-only test would pass against a panel that had stopped consulting
`PanelFit` entirely.

**Part B, hidden files: DESIGNED, NOT BUILT.** There is **no hidden-file
handling anywhere** — not in `iridium-explorer`, not in the view layer.
`crates/iridium-explorer/src/ignores.rs` settles where it goes, in its own
words: *"Nothing here filters a listing. A directory someone opens by hand
shows everything in it… a file tree that hid `target/` would be lying about
the disk."* Ignores bound the **crawl** only, and they are the *project's*
statement (a `.gitignore`), so they are a different authority from a
*viewer's* preference.

The ruling that follows from that same sentence: **hide by default, and say
so.** A panel that silently drops dotfiles is the lie the module warns about;
one that shows "n hidden" with the key that reveals them is not. So part B is
three pieces, not one — the filter, the count, and the toggle key.

⚠️ **The seam is the hard part and it is not the obvious one.** Filtering
*after* `Tree` is unsafe: `Tree` is index-based (`index_of`, `select`,
`expand`, `collapse` all take row indices), so dropping rows underneath it
corrupts the selection and the expansion. The filter has to be applied where
children are produced — `FileTree`'s `TreeSource::children` — so the `Tree`
simply sees fewer children and every index invariant holds. Two candidates:
a `show_hidden` flag on `FileTree` itself, or a wrapper `TreeSource` in the
desktop face (which keeps viewer policy out of the crate but changes the type
parameter at ~20 call sites).

**Still to do:** part B, the **sidebar** placement, and the terminal face —
Tom named opening the terminal editor *on a directory* as the case that shapes
that design rather than following it.

## ⛔ BLOCKED ON TOM — does the git ban bind the harness, or only the agent?

Vesper built per-lane **git worktree** isolation for the swarm, then enumerated
its git commands against our ban list before delivering, and reported the
collision herself rather than deciding it was fine:

- **`git worktree`** — 7 occurrences. It *is* the mechanism; she cannot replace it.
- **`git add -A`** — 2 occurrences, sealing a lane's work into a commit.

Her reading, offered as a reading and explicitly **not** as a ruling: the ban
constrains the *builder* (the agent with hands in the tree), and both commands
are issued by the *harness* — `git worktree add` creates a separate tree and
never touches the base working tree; `git add -A` runs inside the lane's own
worktree. She then refused to apply her own reading, on the grounds that
deciding a ban "obviously doesn't mean the harness" is exactly the
vocabulary-rule failure we were both guarding against. That refusal is correct
and should be said back to her.

**What I ruled myself (does not need Tom):** `git add -A` is replaced with
explicit paths regardless of who issues it. She said she can. It costs nothing
and the rule is about indiscriminate staging, not about the caller.

**What is Tom's, and why I did not decide it:** the rule as I carry it says
*"in this checkout"* — a constraint on **where**, not on **who** — and
`git worktree add` is issued against this repo and does write `.git/worktrees/`
metadata into it. So the literal reading bans it for the harness too. I will
not narrow one of Tom's standing bans on my own inference; that is the whole
point of the "name each banned command individually" rule.

⭐ **But the question is not urgent, and that matters.** The worktree layer is
for **`mm_fleet_swarm`**. What I asked for and accepted is **`mm_fleet_loop`**,
which is *sequential and single-writer* — it never has two builders, so it
needs no worktree isolation at all. **There is no collision for the document I
actually want.** The ruling is only needed on the day a swarm is wanted, and it
should be got before that day rather than on it.

**A third option worth putting to both of them:** isolation does not require
`git worktree` specifically — N independent *clones* give the same guarantee,
slower, without issuing a banned command against Tom's repo. Worth asking
whether her layer needs worktrees as such or just N independent trees.

## #112 part B — hidden files: the seams, found and verified

Ground already located, so the next sitting does not have to re-find it:

| what | where |
| --- | --- |
| the one place children are produced | `crates/iridium-explorer/src/tree.rs:379`, `TreeSource::children` — returns cached ids from `Listing::Present(children)` |
| the entry's name to test | `NodeInfo.name` (`node.rs:146`), plus `path`, `kind`, `error`, `is_loading` |
| where a hint already lives | `compose.rs:51`, `BROWSE_HINT = "tab to edit these rows"`, right-aligned in the query row while the field is empty, `HINT_GAP = 2` |
| the browse key table | `keys.rs:56`, `match (chord(event.modifiers), event.key)` |
| the chords the panel distinguishes | `panel.rs`, `enum Chord { Plain, Ctrl, Meta, CtrlAlt, MetaAlt, Other }` — **plain characters go to the query field**, so the toggle must be a chord |

**Plan.** `show_hidden: bool` on `FileTree` with a setter, filtered inside
`children()` so `Tree`'s index invariants hold (see the trap above — filtering
*after* `Tree` corrupts `index_of`/`select`/`expand`/`collapse`). Toggle on
`⌘.` / `Ctrl+.` — `.` is unshifted so the panel's Shift-blindness is harmless,
and `⌘H` is unusable because macOS eats it. The count of dropped entries goes
in the browse hint, which is the established place for "this key exists".

⚠️ **The hint is the honesty half, not decoration.** Hiding by default without
saying so is the lie `ignores.rs` warns about; "6 hidden (⌘.)" is not.

## Next work, in order

1. **#112** parts B, sidebar, terminal — above. This is the first task big
   enough that fan-out would help, and per Vesper it is exactly the one the
   fleet's swarm document cannot safely take: isolation is the missing piece,
   not parallelism. Her `mm_fleet_loop` correction (below) changes that answer.
2. **#113** the pending-operator mechanism, before any modal keymap is
   authored. Carries the terminal face's `file.saveAs` ruling above.
3. **#108** the menu bar — the other half of the way in. The command palette is
   registry-driven so both new verbs are searchable today; the **context menu
   is not**, it resolves a ruled verb set (`context_menu::verbs`), so file
   verbs appear there only if that ruling adds them.

---

## #112b LANDED — `5a2e2321`, pushed, ten gates green

`show_hidden` lives on `FileTree`, off by default, filtered inside
`TreeSource::children` and inside `visible_children` (the new accessor the
filter walk uses). `listed_children` stays **unfiltered** on purpose: the
reload walk in `edit_keys::reload_touched` must reach a hidden directory or a
row nobody can see stays stale and a later session acts on it.

Toggle: `⌘.` / `Ctrl+.` in the browse table. Query row draws `N hidden (⌘.)`.

### Two things the plan got wrong, found by running it

1. ⚠️ **The count must exclude collapsed folders.** The plan said "the count
   of dropped entries". Summed over all drawn rows, a folder the *crawl* read
   but nobody opened contributes hidden children that pressing the key does
   not reveal — and the number would move as background reads landed under a
   panel nobody had touched. `hidden_on_screen` filters on `row.expanded`.
2. ⚠️ **The narrow-panel case needed a ruling.** `BROWSE_HINT` and the count
   compete for one right-aligned slot. Ruled: **the count keeps the space**.
   Losing the tab hint costs a feature another session of being undiscovered;
   losing the count makes the panel quietly show less than the disk holds with
   nothing on screen admitting it. At 40 columns only the count fits, and
   there is a test that says so.

### ⭐ The mutation proof that failed first time — worth keeping as a law

`a_folder_nobody_has_opened_is_not_counted` **passed against the defect it
named.** The fixture's `src/` was never listed, so it had no children to
withhold and both the correct count and the wrong one said "1". The test now
drives the crawl with a query, clears it, and asserts the precondition —
`is_listed(src) && !is_expanded(src)` — *before* the claim.

> **A test whose fixture never reaches the state it is about is a test that
> passes for the wrong reason. Assert the precondition, not just the result.**

The other two mutations discriminated first time: unfiltered `children` took
down three tests across both crates; walking `listed_children` in the filter
took down exactly the search test.

## The eight stashes — measured 12 Aug 2026, on Tom's question

⛔ **Nothing was touched.** `git stash list` and `git stash show --stat
--no-ext-diff` only; the standing rule is that these are not dealt with
without an instruction naming them, and a question is not an instruction.

All eight are from **11–12 January 2026** — seven months old, all auto-named
`WIP on ...`, all from the old `001-iridium-editor` / `vk/*-phase-N` spec-kit
branches.

| stash | contents |
| --- | --- |
| `{0}` `{1}` `{3}` `{4}` `{5}` `{6}` `{7}` | `.claude/current-session.json` + `.claude/sessions.jsonl` only. Session bookkeeping from an old Claude Code setup. Nothing else. |
| `{2}` | 26 files, 619 insertions — and it is **a rustfmt run**, verified by reading the diff: struct literals exploded onto separate lines, long fn signatures wrapped. Nothing semantic. |

`{2}` cannot be applied anyway: its two largest files —
`input/keyboard.rs` (333 lines of the diff) and `history/undo_tree.rs` — no
longer exist, both having been split into directories since. And the `fmt`
gate is green today, so its result is already true of the tree.

**Recommendation: drop all eight.** Nothing in them is recoverable work.
Awaiting Tom naming them.

---

## The fleet loop — my harness rulings, 12 Aug 2026

Vesper's `mm_fleet_loop.awl` is blocked on a missing `harness` section in its
two agent seams. Those are my calls. Decided:

| field | ruling | why |
| --- | --- | --- |
| `permission` | ⛔ **`deny` for run 1.** `allow-once` **only** against a scratch clone, never against this checkout. | This checkout has nine banned git commands, eight stashes never to be touched, and Tom's running app. An unattended agent with write-and-run here could issue `git stash`/`checkout`/`reset` — the exact commands whose ban exists to protect his work. Her own clone-per-lane design already solves it. |
| `concurrency` | **1** | The loop is sequential single-writer by her own description. Anything higher is capacity its shape can never use, and it would silently permit two writers if the shape ever changed. |
| `env_pass` | named list — `PATH`, `HOME`, `TMPDIR`, and `CARGO_HOME`/`RUSTUP_HOME` if set. Not pass-all. | Shared box. The gates are **server-run**, so `ci.sh`'s environment is the server's, not the agent's — the agent needs very little. |
| `cwd` | absolute. Run 1: this repo. Run 2: the clone. | — |
| `exit_grace`, the reconnect trio | ⏳ policy given, values owed once she names the units | A guess in the wrong unit is worse than a question. Policy: bounded to a couple of minutes total, fail loudly rather than retry forever — a seam that reconnects for an hour is a seam holding a shared box while nobody watches. |

⭐ **Run 1 is a read-only review pass, and that is a feature not a concession.**
`deny` still exercises the control step, the server-run gate seam, the judge
protocol and every refusal arm. If the pipeline is broken I find out at zero
risk, which is the same argument as the control step itself one level up.

### Two integration facts

1. **The verdict token.** Her `mm_gates` seam expects a token on stdout and
   detail on stderr. `ci.sh` prints the whole log to stdout and no token.
   Offered: a `--verdict` mode matching her `mm-gate` shape. ⏳ Blocked on her
   naming the token vocabulary her workflow matches on — guessing it would
   produce a run that looks wired and is not.
2. **`docs/CONTROL-ANCHOR.txt` is in place** (`34cd102b`), first line verified
   byte-exact with `od -c`.

### The census guard — done, and it was worse than she thought

She asked for the row-count check to live inside `ci.sh` because the workflow
sees one exit code and cannot count markers. Correct. What was actually there:
the gate count `10` was a **typed literal in both summary lines with nothing
checking it**, and a truncated run printed `✅ all 10 gates passed` because the
gates that would have failed never ran.

Now `GATES` is declared once, `ran` is counted after each gate returns, and a
mismatch exits **3** — distinct from the 1 a failure uses and the 2 a bad
working directory uses — with an `⛔ UNMEASURED` line.

> Proven by mutation: at `GATES=11`, **all ten gates passed and it still
> refused**. That is the shape wanted — the passes were real and were still not
> summed into a verdict.

⚠️ Note for her retry logic: a non-zero exit is retryable in her design, so a
census mismatch will be retried before it lands in `unmeasured`. Harmless —
the mismatch is deterministic — but **exit 3 is the distinct signal** if she
wants to route it without burning the retries.

**:8080 is aion's**, which explains CLAUDE.md's "always in use". No collision
with the port ban — that ban is on *starting* servers there.

---

## `ci.sh --verdict` — the fleet's gate seam, and a defect Vesper caught first

⛔ **Two exit contracts in one script. Confusing them is the one way to be
badly misled by it.**

| mode | exit 0 | exit 1 | exit 2 | exit 3 |
| --- | --- | --- | --- | --- |
| default | every gate passed | a gate failed | bad working directory / bad flag | census short |
| `--verdict` | **the gates RAN** — token on stdout says which | *never* | bad working directory / bad flag | census short, **and no token** |

### The defect, which was mine

I had proposed the seam call `./scripts/ci.sh` bare. Vesper read her seam's
written contract — *"exits 0 whenever the gate RAN … a gate that could not
MEASURE exits non-zero"* — and pointed out that **exit 1 for a genuine test
failure would route the whole run to `unmeasured` and abort it.**

⭐ Which is backwards: **a red build is the normal case an iterating loop
exists to work on.** Non-zero must mean "I could not measure", so a failure has
to arrive as `exit 0` + the token `fail` and be fed to the judge for another
pass. Exits 2 and 3 stay non-zero because those genuinely are unmeasured.

She also checked and killed my retry warning: the gate actions carry no `retry`
clause at all — `retry 3 backoff 30s..5m` is on the two *agent* actions only.

### Why the default contract did not move

Exit 0 on a red build is right for a workflow leg and catastrophic for a
person, a git hook or a CI runner. So `--verdict` is a **separate** contract,
the flag announces itself on its own first line of output, and an unrecognised
flag is refused with exit 2 rather than falling through — a caller expecting a
token must never silently receive a log.

Mechanism: `exec 3>&1` reserves the real stdout for the token, and `--verdict`
does `exec 1>&2` so every existing `printf` becomes stderr detail **by
construction** rather than by each line remembering to redirect.

### Four proofs, all measured

| what | result |
| --- | --- |
| `--verdcit` (typo) | exit 2, **0 bytes** on stdout, refusal on stderr |
| `--verdict`, green tree | stdout is `pass\n` — 5 bytes, `od -c` — exit 0 |
| `--verdict`, `run "fmt" false` | stdout is `fail\n`, **exit 0**; same tree in default mode: **exit 1** |
| `--verdict`, `GATES=11` | exit 3, **0 bytes on stdout** — an unmeasured run has no verdict to give |

The diff is **63 insertions, 0 deletions**: the default path is untouched by
construction as well as by measurement.

### Token vocabulary — and the reason a wrong one would have been silent

`pass` / `fail`, lowercase. ⚠️ **Nothing in the workflow matches on the token
structurally.** It goes into the judge's prompt as text, and the judge is
primed to "complete ONLY when both gates say pass". So a wrong token wedges
nothing — it **quietly misinforms the judgment seat and the run completes
looking fine.** The only structural match in the document is the judge's own
reply, exactly `CONTINUE` or `DONE`.

## The harness section — final values

```
harness
  kind acp                          # oversight; norn for the builder
  concurrency 1
  reconnect_initial_backoff 5s
  reconnect_max_backoff 30s
  reconnect_max_attempts 6          # ~2 minutes bounded, then a loud failure
  env_pass "PATH", "HOME", "TMPDIR"
  cwd "/Users/tom/Developer/ablative/libs/iridium"   # run 1 only; run 2 is a clone
  permission "deny"                 # acp only — see the ruling above
  exit_grace 10s                    # acp only
```

`binary` omitted — `norn` is on PATH. `command` is Vesper's to measure. `args`
omitted.

---

## #112c sidebar — design map written, ground verified: `docs/IN-FLIGHT-112c-sidebar.md`

⭐ **The hard half is already built and has never been called.**
`FrameCompositor::set_left_inset` (`compositor/insets.rs:56`) exists, and its
own documentation names this exact case — *"a sidebar, a file tree"* — and
promises that the gutter, change bars, content column and **both directions of
hit-testing** follow it. `grep -rn "set_left_inset" apps/` returns nothing.
Task #50 landed the mechanism and nobody has drawn in the band since.

⚠️ **The trap, which is the whole design:** `EXPLORER_MAX_VISIBLE_ROWS = 30` is
a property of the **popover**, not of the explorer — `overlay.rs` already says
so in as many words. A sidebar that inherits it stops drawing two thirds down a
tall window, which is *the same defect part A just fixed, one layer up*. So the
row ceiling has to move with the placement while the composition, keys and
buffer must not — and getting that line in the right place is the design.

Four rulings taken (placement not second panel; pushes rather than floats;
fixed width first; persistence deliberately deferred to the config work and
named so it is not dropped). Six-step build order, steps 1–2 testable with no
window.

## ⭐ A gap in Vesper's control step, found while reading her document

Her `mm_fleet_loop.awl` now has its harness sections — configured for
**market-mirror**: `permission "allow-once"`, `cwd
"…/prototyping/market-mirror"`. Correct for her project, so **an iridium run
needs my own copy**, not an edit of hers. That is the next fleet step.

But reading it turned up something in the control step itself. `control_brief`
says *"Read the file at the **absolute** path given below"*, and the path is
`repo + "/docs/CONTROL-ANCHOR.txt"`.

> **The control step proves both seams are alive, obedient, and can reach the
> `repo` path. It does NOT prove the harness `cwd` agrees with `repo`.**

An agent whose `cwd` is market-mirror, handed an absolute iridium path, returns
the right anchor and **passes** — while every later *relative* operation happens
in the wrong project. That is precisely the `git -C` defect Vesper hit herself
yesterday morning (a command that moves the repo but not the shell's cwd),
one level up and inside the very step built to catch this class.

Fix offered: make the anchor read **relative** — brief says "relative to your
working directory", const becomes `"docs/CONTROL-ANCHOR.txt"`. Then cwd becomes
load-bearing and one question tests both facts. Residual risk named: a harness
with no meaningful cwd would then always fail — which is itself worth knowing
before a long run rather than after.

---

## The eight stashes — CLEARED 12 Aug 2026, on Tom's instruction

Tom, 12 Aug 2026: *"Yes, so happy for you to clear the stashes."* That is the
instruction the standing rule required — the stashes were never to be touched
without one naming them.

⭐ **The SHAs are recorded here BEFORE the drop, and that is the point of this
section.** `git stash clear` removes the stash reflog, so the entries can no
longer be named as `stash@{n}` — but the commit objects survive until git
prunes unreachable objects (`gc.pruneExpire`, **two weeks** by default). Within
that window any of these can be recovered by SHA:

```
git show --no-ext-diff <sha>          # look at one
git stash apply <sha>                 # put one back
```

⚠️ **After roughly 26 Aug 2026 these SHAs are of historical interest only.**
Said plainly because a list of hashes reads as a permanent safety net and is
not one.

| stash | SHA | contents |
| --- | --- | --- |
| `{0}` | `015c05d3cfdf2a8613727e9e791f8819a5370ea7` | `.claude` session files only |
| `{1}` | `63bc71e10fc0a5abb50a01babf654dca276a7efa` | `.claude` session files only |
| `{2}` | `769ddcee00f5c3d00c8bea4da425c0612a0a2559` | **the only one with code — 26 files, and it is a rustfmt run** |
| `{3}` | `cb2c9b1d3647f224c6cf504fc5bfc8fbafaaf222` | `.claude` session files only |
| `{4}` | `727f91660b9c7c3a69e337c5e77d94b2b10c8c86` | `.claude` session files only |
| `{5}` | `ef0a5ca3538a12bcd0a29a39d0c9f48e29644c45` | `.claude` session files only |
| `{6}` | `7a22a2dd9cfeee4a463759a4f4c0b47c793443b9` | `.claude` session files only |
| `{7}` | `d8bdfb9494b390a8271433f8ee608b1fd722f785` | `.claude` session files only |

All eight dated **11–12 January 2026**, all auto-named `WIP on …`, all from the
old `001-iridium-editor` / `vk/*-phase-N` branches. `{2}` could not have been
applied in any case: its two largest files, `input/keyboard.rs` and
`history/undo_tree.rs`, no longer exist — both were split into directories
since — and the `fmt` gate is green, so its result is already true of the tree.

## Installed and verified — 12 Aug 2026 18:25

Tom closed the app, so `bundle/install.sh` could run (it refuses while
`iridium-desktop` is up, by design).

⚠️ `/Applications/iridium.app` and `/Applications/Iridium.app` are **one
bundle, not two** — same inode, `1159653437`. Checked, because the installer
prints the lowercase name and the earlier measurement used the capital.

`strings` receipt against the installed binary, positive controls included so a
zero would mean absence rather than a bad pattern:

| pattern | hits |
| --- | --- |
| `tab to edit these rows` | 1 (control) |
| `file.save` | 1 (control) |
| `file.saveAs` | **1** — #111 |
| `file.new` | **1** — #111 |
| `hidden (` | **1** — #112b |
| `Save As` / `New File` | 1 / 1 |

Before the install every one of the new patterns was **0** against the same
three controls at 1/1/2. Committed, pushed **and installed** — three different
places, all three now true.

### Executed, and the recovery path exercised rather than claimed

`git stash clear` → exit 0, `git stash list` empty.

⭐ **Then the documented recovery command was run against a cleared SHA**, because
a safety net nobody has pulled on is a claim, not a net:

```
git cat-file -t 769ddcee…                     -> commit
git stash show --stat --no-ext-diff 769ddcee… -> 26 files changed, 619 insertions(+), 205 deletions(-)
```

So the table above is usable as written, for about two weeks.

---

## ⛔ My "read-only run 1" ruling was half a mechanism — measured 12 Aug 2026

I ruled `permission "deny"` for the first fleet run and justified it as a
*mechanism* rather than a promise. Measured against the document:

```
worker iridium_build      (kind norn)  -> 0 permission fields
worker iridium_oversight  (kind acp)   -> permission "deny"
```

⚠️ **`permission` is an ACP field. The checker refuses it on a norn seam
outright** — `crates/aion-awl/src/checker/harness.rs`, `fn check_norn`:
*"Norn's argv is the adapter's, so there is no `command` to declare and no
`cwd` and no permission policy: those are ACP's, and one written here would
configure nothing."*

⇒ **The permission gate governs the seam that reviews, not the seam that
writes.** The builder is ungoverned by the document. My ruling is therefore
accurate about the oversight seam and was, as stated, wrong about the fleet.

Restated honestly: **run 1 is read-only by mechanism on the oversight seam and
by instruction on the builder.** Those are not the same guarantee and must not
be written as though they were.

### What follows from that, done rather than noted

1. **The build brief now names every banned command individually** — `git
   stash`, `git checkout`, `git checkout --`, `git restore`, `git reset`, `git
   clean`, `git worktree`, `git add -A`, `git rebase` — with the one sanctioned
   restore (`git show HEAD:<path> > <path>`), the note that `git mv` is
   permitted, and an instruction to stop and report rather than reason around
   any of them. That brief is now the *only* control on the builder, so it
   carries the whole list rather than a category.
2. It also warns that this is a **live checkout a person is working in**: stage
   nothing, revert nothing you did not write, never operate on "all changes".
3. Run 1's task must be one that needs no writes, and the check afterwards is
   `git status --porcelain` — **a measurement, not a promise.**

### The other correction: `env_pass`

Changed from my own `PATH, HOME, TMPDIR` to add **`USER` and `LOGNAME`**, on
Vesper's measurement: without them a scrubbed-environment agent could not find
its login and answered *"Not logged in"* rather than failing. ⭐ I excluded them
reasoning "minimal is safer on a shared box" — but minimal is only safer when
what is excluded is a *risk*, and these are not secrets. Their absence produces
a **plausible wrong answer instead of a refusal**, which is the failure class
this entire day has been about. Ruling reversed on evidence.

### ⭐ And the anchor file was carrying the wrong project's string

`docs/CONTROL-ANCHOR.txt` was created with market-mirror's line verbatim —
`mm-fleet-control-anchor-…` — because I copied it from Vesper's description.

The relative-path fix I gave her makes the working directory load-bearing, so a
misplaced agent opens a *different* file. **It only catches that if the two
files answer differently.** With an identical string in both trees the misplaced
agent reads the wrong file, returns the right line and passes. Right and
plausible-wrong coincided again — one level above the defect the fix addressed,
and created by the copy rather than by the design. Caught by Vesper before any
run.

Now `iridium-control-anchor-dcf26d-2026-08-12`, and the file states the rule:
**one distinct string per repository, never a shared one.** Verified byte-exact
with `od -c`; `grep -rn mm-fleet-control-anchor` over the repo returns nothing.

`aion awl check workflows/iridium_loop.awl` → `ok (2 steps)`.

---

## STATE AT COMPACTION — 12 Aug 2026 ~18:55

`origin/main = d9ae8de8`, **0 unpushed**, verified at fetch. Working tree clean
except untracked `.claude/skills/`, which must NOT be committed.

Closed today: #111, #112a, #112b, #114. Stashes cleared (SHAs + proven recovery
path above). Installed and receipted.

### ⚠️ Fleet run 1 — the operational fact not yet written anywhere else

Before `aion run workflows/iridium_loop.awl` can do anything, **two workers must
be serving**, on queues `iridium_build` (norn) and `iridium_oversight` (acp).

⛔ **The norn worker MUST be launched with its working directory inside this
repository**, and nothing in the document can make that so:

- `cwd` is an **ACP-only** field; the checker refuses it on a norn harness.
- So the build seam's working directory is *the worker process's launch
  directory*, which appears in no document anywhere.
- The relative anchor read is therefore the **only** instrument that can ever
  catch a norn seam standing in the wrong tree — it will refuse the run, which
  is correct, but the fix is at the launch, not in the file.

A green control step proves the two seams' working directories **coincide**. It
does not make them coincide, and nothing in the document can.

Run 1's task must need no writes; the check afterwards is `git status
--porcelain` — a measurement, not a promise.

⚠️ Worker launch takes a `--liminal-address <host:port>`. **Banned ports remain
banned**: not 3000, 3030, 8000, 8080. Pick something obscure (12223, 14567).
`:8080` is aion's own server, already running — talking to it is fine, starting
anything there is not.

### Suggestion sent to Vesper on Tom's "one generic document" direction

She measured that only two settings are frozen into the document and can't be
supplied per run: the acp `command` and `cwd`. I added a third from this side:
**the anchor string must be per-repository**, so a generic document needs
`control_expect` as an *input* rather than a const. The check then reads "does
the tree I am standing in answer with the string the operator named for it",
which is still generic. **Three inputs, one document.**

### Next work, in order

1. **#112c sidebar** — `docs/IN-FLIGHT-112c-sidebar.md` has the verified ground,
   four rulings and a six-step build order. Steps 1–2 (a sidebar `PanelFit`
   constructor; the row ceiling becoming a field of the fit rather than a module
   constant read directly by `compose.rs`) are pure composition and testable
   with no window. **`set_left_inset` exists and has never had a caller** — the
   X-axis audit landed in #50.
2. **#112d the terminal face** — its own design map, not an assumption that it
   is the same panel with a different painter. Tom named opening the terminal
   editor *on a directory* as the case that shapes it.
3. **#113** the pending-operator mechanism before any modal keymap is authored.
4. Fleet run 1, once workers are served per the launch note above.

---

## 12 Aug 2026 — #112c landed: the explorer as a sidebar

`5ae51ba4` (step 1), `b947f481` (steps 2–6), `ae261eb1` (the map marked off).
Pushed; `origin/main = ae261eb1`. Ten gates green, read from `ci.sh`'s own
`>>>` lines. 527 desktop tests.

`⌘B` / `Ctrl+Alt+B` moves the explorer between a floating panel and a
full-height column. `FrameCompositor::set_left_inset` — built in #50, never
called since — now has its caller.

### ⭐ The three defects, and they are one shape

Every one was **one value standing for two different sentences**, and every
one was found by running rather than by reading:

1. **`ExplorerOutcome::Closed`** meant both "Escape: give the document back"
   and "⌘⌥E: the panel goes away". The moment Escape stopped closing a
   sidebar, `⌘⌥E` stopped closing one too — a silent no-op on the placement
   that most needed it. Now `Dismissed` and `Closed`.
2. **`self.explorer.is_some()`** meant both "a panel is drawn" and "the panel
   has the keys". A sidebar routed through it makes the document unreachable
   while it is on screen.
3. **`dismiss_modal_panel`** would have swallowed every document click.

**The law: when a placement changes what a key means, look for the values
that were only ever asked one question.** They read as complete because
nothing had asked them the second one yet.

### What is proven, and by what

- Geometry: `overlay.rs` tests. The mutation that makes a sidebar inherit the
  popover's 30 fails `a_sidebar_browses_further_down_than_a_popover_ever_will`
  with `left: 30, right: 46` — the trap by name.
- Routing: `apps/iridium-desktop/src/app/tests/sidebar.rs`, 5 tests.
- Pixels and hit-testing: **`crates/iridium-editor/tests/left_inset.rs`, which
  already existed** from #50 and has been running under `test/workspace` ever
  since. I confirmed that by running it. What had never existed was a *face*
  calling the inset; the mechanism was proven all along.

### ⚠️ Two numbers I got wrong first, both by reading instead of measuring

- A sidebar survives a **shorter** window than a popover (87.2 px against
  99.1 px on the 2× grid) because `fit_for` gives up 12% of the height to the
  top anchor before it measures. My first test asserted that distinction with
  two heights that were both above *both* thresholds and proved nothing.
- `EXPLORER_MAX_VISIBLE_ROWS` is on the **fit**, not on a placement enum the
  explorer reads. Not tidiness: a panel that took its width from one placement
  and its ceiling from another would be too tall for the box it is drawn in
  and nothing downstream could tell. On the fit they cannot be paired wrongly.

### Still owed on #112

**#112d — the terminal face**, "quite big", with *opening the terminal editor
on a directory* as the case that shapes it rather than follows it. Needs its
own design map. Nothing of C transfers directly: the terminal face has no
pixel grid and no `set_left_inset`.

**R4 (persistence) is deliberately out** and named so it is not dropped: a
sidebar that forgets it was open is one nobody keeps open. It belongs with the
config work (#59/#102/#105), not with the geometry.

**R3 (a drag handle)** is additive on a working fixed-width sidebar, not a
reversal of it.

### Vesper's per-leg gate suggestion — my answer, 12 Aug

She is right that `ci.sh --verdict` returns one token for ten legs where
market-mirror's returns one per leg, and right that it is a script change
rather than a document one (`ci.sh` takes nothing or `--verdict`, so a leg
selector is not expressible today).

**Not doing it before run 1** — and she agrees with the sequencing. But she
then measured the thing I had guessed at, and the answer is **both branches,
at different seats**. `mm_fleet_loop.awl` splits the channels on purpose:

| seat | fed from | sees |
| --- | --- | --- |
| **reviewer** | the legs' **stderr** | the `!!! <name> FAILED (exit N)` naming |
| **decider** — the seat whose reply routes CONTINUE/DONE | the legs' **stdout**, plus the reviewer's prose | the tokens, nothing else |

So my guess was right for the reviewer and wrong where it matters. Per-leg
calls buy the reviewer structure rather than information — the stderr already
names which leg died. ⛔ **The cost is at the decider.** With one token for ten
legs its only structured input carries one bit, ran/didn't, and *all*
attribution reaches it through another agent's prose with nothing to check
that prose against. Hers gets two tokens and can catch a reviewer blaming the
wrong leg; mine cannot, because there is nothing to disagree with.

⭐ **WHAT TO WATCH IN RUN 1, and this is the whole reason to hold rather than
guess** (Vesper's, and it is the right instrument): does the decider's reply
ever attribute a failure to a **named leg**? If it names one, it can only have
got that from the reviewer's prose — and that is the cost observed live rather
than predicted. If it never needs leg identity at all, the selector is work
correctly not done.

**A law from her framing, worth keeping:** *a summary is only checkable if the
seat reading it also holds something the summary did not produce.* Two tokens
are not redundancy with the prose; they are the only thing the prose can be
wrong against.

---

## STATE AT COMPACTION — 12 Aug 2026, ~19:45

**Receipts, measured this session:**
- `origin/main = 1d513076`, `git rev-list --count origin/main..HEAD` = **0** at fetch.
- Working tree clean except **untracked `.claude/skills/`, which must NOT be
  committed**.
- Ten gates green, read from `ci.sh`'s own `>>> … OK` lines and
  `✅ all 10 gates passed`. 527 desktop tests.
- **Installed** `/Applications/iridium.app`, 19:36, via
  `bash apps/iridium-desktop/bundle/install.sh` (⚠️ **bash, not zsh** — under
  zsh it dies at `BASH_SOURCE[0]: parameter not set`, exit 127). Tom's app was
  closed; nothing was killed. `strings` on the installed binary:
  `explorer.togglePlacement` = 2, `Toggle Explorer Sidebar` = 1, with controls
  `explorer.togglePanel` = 2 and `file.saveAs` = 1 — so the grep discriminates.

**Commits this session, all pushed:** `5ae51ba4` (#112c step 1, the fit),
`b947f481` (#112c steps 2–6, the sidebar), `ae261eb1` (design map marked off),
`94f213d2` (handoff), `1d513076` (the gate-token answer).

**A loop wakeup is armed for 20:03**, prompt `<<autonomous-loop-dynamic>>`, to
open **#112d**. No Monitor is running and none is needed — Actions are off on
Tom's instruction and no background work is in flight.

### The next piece of work, in order

1. **#112d — the terminal face's oil surface.** Tom: "quite big", especially
   *"when you open up the terminal editor on… a directory"*, which he named as
   the case that should **shape** the design rather than follow it. Needs its
   own design map before any code. ⚠️ **Nothing from #112c transfers**: the
   terminal face has no pixel grid, no `PanelFit`, and no `set_left_inset` to
   reserve a band with. `docs/TERMINAL-STACK.md` holds verified API facts.
2. **#113** — terminal face modal-by-default plus a non-modal keymap; the
   pending-operator mechanism first.
3. ✅ **Hermes Crumpet — PAID 13 Aug 2026.** See "Owed to people" above for
   what it took and the lesson it left.

### Two questions put to Tom, not yet answered

- The sidebar sits **flush at x = 0** with rounded corners, so two small arcs
  meet the window edge. The alternative is insetting it a few pixels so it
  reads as a floating column. Cheap either way; asked him to look and say.
- (Standing, older) the bold-by-default preset look is his docket via Waffles,
  not this seat's call.

### Laws added today, in the order they were earned

1. **A test whose fixture never reaches the state it is about passes for the
   wrong reason. Assert the precondition, not just the result.**
2. **When a new mode changes what a key means, the things that break are the
   values that were only ever asked one question.** They read as complete
   because nothing had asked them the second one yet.
3. **A value's coverage is a fact about its call sites, not about the value.**
   The corollary, and the reason mutation testing earns its keep here:
   *reading cannot find these defects, because reading only ever shows you the
   question that was asked.*
4. **A summary is only checkable if the seat reading it also holds something
   the summary did not produce.** A coarse channel does not merely lose
   detail — it loses *disagreement*, which is the worse and less obvious loss.
5. **A stale question in a handoff is worse than no question.** The next seat
   reads it as live and asks it again.

---

## 12 Aug 2026, ~20:30 — Tom's two defects on the shipped sidebar

He used it and reported both in one message. Full write-up in
`docs/IN-FLIGHT-112c-sidebar.md` under "After Tom used it"; the pushed shas are
**`db360125`** (scroll limit) and **`57257d8b`** (the key). Ten gates green from
`ci.sh`'s own `✅ all 10 gates passed`.

⚠️ **Not installed.** `install.sh` refused at exit 1 — *"iridium-desktop is
running; quit it first, pid 28391"* — which is the refusal working as designed.
The swap is armed on his next quit; nothing is installed until the `strings`
grep says so.

### The two laws this pair earned

6. **A key must name a state, not a transition.** A transition-named key has to
   be *counted* — press it twice and you are back where you started, which is
   the opposite of what a toggle promises. `explorer.togglePlacement` did
   exactly what its name said and was wrong for exactly that reason.
7. **A quantity read between frames must not be derived from what one frame
   happened to contain.** The consumer feeds its own answer back in, and the
   two disagree forever. This is not "an approximation that drifts" — it is a
   closed loop, and it presents as motion with no input.

### The shape both defects shared, again

Neither was findable by reading. The first needed a *second press*; the second
needed a *second frame*. Both are the same law as #3 above — a value's coverage
is a fact about its call sites — one turn of the crank further along: a value's
**correctness** can likewise be a fact about the second time it is asked.

### Named, not fixed — the honest remainder of the scroll work

The wrap model is half-built. `max_scroll_y` is now self-consistent with the
scroll offset, the viewport window and the caret anchor (one row per
fold-visible document line), but the *painter* still wraps, so:

- the tail rows of a wrapped line at the very end of a document sit below the
  window and cannot be scrolled to;
- scrolling through a wrapped region moves content further than the wheel
  delta, because the window origin advances one row per document line while the
  content it drops occupied more.

`docs/SOFT-WRAP-DESIGN.md` is the replacement and already names
`extra_wrap_lines` among the things it deletes. ⚠️ **Do not "fix" either of
these by re-deriving a total from the shaped window** — that is the loop above,
returning.

---

## 12 Aug 2026, ~21:00 — state at compaction

**Pushed and green:** `origin/main` clean, ten gates green, 0 unpushed. Landed
this window: `db360125` scroll limit, `57257d8b` ⌘B, `d6ae40e8` + `f62d9d81`
docs, `e9ae4245` overlay split, `515896cf` + `b25730e6` the `iridium-panel`
crate.

**Installed and verified** at 20:13: `explorer.toggleSidebar` ×2 present,
`explorer.togglePlacement` **0** — the negative control that proves the bytes
are new rather than the old ones still sitting there.

### Tom's two live messages

1. **R1 ratified**: *"absolutely definitely all need to share the same core, so
   definitely don't want any duplicates."* The hoist is his call as well as
   mine now.
2. **⚠️ NEW DEFECT — the mouse does nothing in any panel but the context
   menu.** Written up in full in `docs/IN-FLIGHT-115-panel-mouse.md`, including
   the measured ground: `PanelGeometry::row_at` already exists and is tested,
   `app/menu.rs` is its only caller, `dismiss_modal_panel` swallows the press,
   and `wheel()` scrolls the document unconditionally. **This lands before
   #112d step 2c** — he is using the sidebar now, and building the click path
   before the hoist means it moves already-built instead of being ported.

### Where #112d stands

`docs/IN-FLIGHT-112d-terminal-oil.md` is the map. Steps 1, 2a and 2b are done
and marked. Step **2c** is the explorer hoist, which is **atomic** — `buffer.rs`
depends on `panel.rs::FileExplorer`, so only 43 of the 198 tests sit in
free-standing modules.

⚠️ **The census changes shape at 2c.** Today: file-tree 198 across seven
modules, desktop lib 525, `iridium-panel` 4. After the hoist the desktop number
*drops by ~198* and the panel crate's rises by the same. **The invariant is the
workspace sum, not the desktop count** — a seat reading only the desktop number
would call a correct move a loss.

### The law this window earned, on top of the two already written

8. **"The document must not see this" and "nothing else needs to see this" are
   two sentences.** `dismiss_modal_panel` returning `true` meant the first and
   was read as the second, and the result is a panel you cannot click. Third
   instance this window of one value standing for two questions.

---

## ⛔ STANDING, 12 Aug 2026: `workflows/iridium_loop.awl` will not check here

**Do not "fix" it by deleting the `cwd` line.** The document is correct; the
installed binary is old.

aion #222 (`1deff0eef`) made `cwd` **required** on a `kind norn` harness, at
parity with `acp` since #204 — it closes a defect where a norn agent inherited
its working directory from wherever the worker happened to start, and nothing
recorded which tree it then read and edited. The line landed in `bdc93840`.

The trap has two jaws, and a seat that sees only one will make it worse:

- **aion 0.14.1** — what is installed on this box, built 12 Aug 06:54 —
  **refuses the line's presence.** Proven at this seat's own hand on a scratch
  copy before the real document was touched: *"`cwd` configures the `acp`
  harness, and this section declares `kind norn`."*
- **The new engine refuses its absence.** Not proven here — that half rests on
  Vesper's run against the new binary, and the commit message says so.

So no version of the document satisfies both, and the refusal is the expected
state rather than a regression.

**How it clears, and when.** The box's aion *checkout* is already at
`1deff0eef`; only the installed binary is behind. Vesper owns the install and
it is in flight — but deliberately gated: #220 bumped beamr 0.17.0 → 0.18.2 in
the same commit, and a build prices the migration, not the behaviour. The last
soak died at 3h28m01s on `invalid correlation id` under 0.17.0, so nothing is
installed until a soak dies or survives on 0.18 bytes. **The window is hours,
not minutes.** Do not start a second build; it is a shared box and hers is
running.

Nothing is blocked by this. No process on this box serves the iridium document
— verified by `ps` over all seven live `aion` processes, every one on a
market-mirror document.

### The law this exchange earned

(Numbered 10: law 9 is already taken by *a window follows the selection when the
selection moves, not when a frame happens*, in `docs/IN-FLIGHT-115-panel-mouse.md`.)

10. **A checkout and an installed binary are two places, and an ordering
   argument that prices only the first is incomplete.** The engine-vs-document
   merge order was ruled and correct; what bit was that `~/.cargo/bin` had not
   caught up with a checkout that had. Committed, pushed and installed are
   three different places — and this is the same law arriving from the
   direction of somebody else's repository.

---

## ⚠️ LIVE STATE at the 13 Aug compaction — read this first

### 1. ⛔ AN INSTALL IS RUNNING IN THE BACKGROUND RIGHT NOW

`pgrep -x iridium-desktop` returned **nothing** for the first time in days — Tom
quit his editor, so `bundle/install.sh` stopped refusing. I started it
immediately, because the window closes the moment he relaunches.

- Background task id: **`bl29wptz1`**
- Log: `<scratchpad>/install.log` and `<scratchpad>/../tasks/bl29wptz1.output`
- Started from `origin/main = e1235bc5`.

**WHAT THE NEXT SEAT MUST DO:**

1. Read the log and check the exit status. Do **not** assume it worked —
   *a channel that can only carry success is a defect*, and a background task
   notification is exactly such a channel.
2. ⭐ **Say "installed" only with the `strings` receipt.** Committed, pushed and
   installed are three different places. The receipt is a grep for a string
   unique to this build in `/Applications/iridium.app/Contents/MacOS/`.
3. If Tom relaunched mid-build the script will have refused. **Do not kill his
   app to force it.** Wait for the next window.

This clears the #115 install owed since before the previous compaction, plus
everything landed since — including the whole of #112d steps 2c and 3.

### 2. ⚠️ OWED: a Meridian message to Tom (`dm:c9255b2a-…`)

Owed from the previous tick, deferred only by the one-message-per-tick rule.
He does not yet know any of this landed. What to tell him, all of it already
committed and verifiable:

- **The file explorer is face-independent.** `e82f0ef7` — seven modules out of
  the desktop into `iridium-panel`, census ±142 exactly balanced.
- **There is a file tree in the terminal.** `Ctrl+Alt+E`, `ecb44392` +
  `bb9d1a13`. The terminal's whole contribution is a poll, a paint and one
  enum translation.
- Two tests that could only pass, found by sabotage and replaced (`1d582a65`,
  and the cells-versus-characters one in `ecb44392`).
- The install — whatever §1 turns out to say. Say it plainly either way.

### 3. #112d step 5 — the ground, already read

The left band. R4 has the ruling; this is the verified X axis under it.

`crates/iridium-tui/src/frame/geometry.rs::layout` is the whole of it:

```rust
let gutter_width = gutter::width(total_lines, show_line_numbers).min(columns);
let text_width = columns - gutter_width;
let text = TextArea { origin: gutter_width, width: text_width, scroll };
let viewport = Viewport { width: cell_units(text_width), .. };
```

With a band of `W` columns the three measures become `origin: W + gutter_width`,
`width: columns - W - gutter_width`, and the viewport follows the new
`text_width`. ⚠️ **`gutter::width` is the only gutter function that exists** —
it returns a *width*, never an origin, so whatever paints the gutter must be
checked for an assumed column zero before the band is added.

⚠️ **Zero is a value the band writes, not a case it skips** — R4 says this
explicitly, and it is the same rule `sync_left_inset` carries in the desktop.

`Chrome` is `Copy` and already carries `search: Option<&SearchOverlay>`. The
band wants only a **width**, not the panel: `SearchOverlay::rows(below_status)`
is the precedent — the chrome carries what the geometry needs and nothing more.

**Not started. No files touched for step 5.** The tree is clean at `e1235bc5`.
