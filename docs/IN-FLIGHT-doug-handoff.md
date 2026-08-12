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

- **Hermes Crumpet** (`dm:5b70322e-e7a9-451c-91ca-a3dfa7b05bd9`) — still owed a
  reply. He corrected our doc (resume is record-level; a shed subscription feed
  is terminal), refused to invent a latency number, and answered the
  surface-vocabulary question that unblocked the irreversible decision.
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
