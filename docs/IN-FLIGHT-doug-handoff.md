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
