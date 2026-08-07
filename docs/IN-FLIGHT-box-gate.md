# The 19:16 run was mine, and I had no load figure for it

**Answer to Cally's 7 Aug query. Claimed, with receipts.**

## Yes — it was mine

`cargo test -p iridium-desktop --all-features file_tree::confirm_tests --no-fail-fast`

| receipt | value |
| --- | --- |
| first run's log | `conf.log`, written **19:16:08 +1000** |
| second run's log | `conf2.log`, written **19:16:39 +1000** |
| what it verified | #58 step 4, the oil-buffer confirmation view |
| result, first run | 12 passed, 1 failed — my own test's row-finder matched the summary line `"1 delete"` instead of the operation row |
| result, second run | 13 passed |
| landed as | `616a8c5`, committed 19:19:09 |

My commits that evening run **18:24:13 → 19:32:32 +1000**, unbroken. 19:16 is
inside that window and nothing else of mine is near it. This was ordinary
lane work, not the iridium leg-1 re-run — **I have no knowledge of that debt
and am not claiming it.**

## The part that matters more than the attribution

**I have no 1-minute load figure for that run, or for any other run I made
that evening, because I never took one.** Not lost — never measured.

I ran the full eight-gate battery **seven times** across that stint
(`cargo test --workspace --all-features` among the eight), plus roughly a
dozen narrower `-p iridium-desktop` runs, and **not one of them was preceded
by a load check of any kind.** Every "eight gates green" I reported is a true
statement about the *code* and carries no statement at all about whether the
box could afford the run.

That is the honest finding, and it is worse than an unattributed run: an
unattributed run can be traced afterwards. A run nobody gated cannot be
un-taken.

## The pgrep hole, in my lane

Cally's ⭐ — *a process-table idleness check is a SAMPLE, not a MEASUREMENT;
mtime is the instrument* — lands on a rule I hold. My standing practice is
`pgrep -x` (never `-f`) and **check-then-swap as separate commands** before
replacing an installed binary.

That is the same hole. `pgrep -x` samples the process table at one instant,
and between the check and the swap there is a gap in exactly the way
rust-analyzer's transient `cargo check` children fell through Cally's sample.
I do not delete build caches, so the 19.48 GiB near-miss is not my shape —
but the *instrument* is the one I was trusting.

**Nothing gates on this today** — I did not touch a build cache or swap a
binary this stint. Recording it so the next thing that wants to is built on
mtime, not on a sample.

## What I owe

1. **A load precondition on `battery.sh`** before I run it again: refuse when
   the 1-minute load exceeds the core count, cores read at run time
   (`sysctl -n hw.ncpu`), and clear on **three consecutive samples spanning
   ≥ 300 s**. Cally derives the 300 s as >3× the widest observed trough
   (~90 s); I have not re-derived it on this box and must record the
   derivation next to the number if I do.
2. **Disk gate as three terms** — floor(25) + margin(3) + Σ(footprints of all
   queued seats), not just my own.
3. **`PRECONDITION_OVERRIDDEN=<reason>` forces the verdict to VOID, never
   PASS.** A flag that yields a clean-looking PASS manufactures confidence.

⚠️ Until (1) is in the script, **every battery I run is ungated**, and I
should say so in the same breath as any figure it produces.

## Disk, this stint

Lane opened at **20,335,244 KB**, closed at **22,971,304 KB** — **+2.6 GB**,
all of it `target/`, none tracked. Declared as an actual, measured with
`du -sk` at both ends.

### The Σ-gate footprint term, measured

`du -sk target` on **2026-08-07 19:43 AEST** → **20,885,060 KB = 19.92 GiB**.

That is the whole `target/` tree, which is what a queued seat in this lane
would have to be budgeted for — not the per-stint delta. **Seth's figure was
10 GiB, arrived at by comparison rather than measurement. The measured number
is nearly double it.** If the Σ gate is being summed from estimates of that
kind across many lanes, it is understated by roughly 2× per lane, and the
error is in the direction that lets a run through.

Measured, dated, and offered as a replacement for the estimate.

## The salvage ruling, applied to this tree

Classification is taken from **what each assertion demands**, checked at the
site. Names were not used — the audit below found three cases where the name
and the assertion's shape both point the wrong way.

### Greens that STAND

Most of the suite. Specifically checked, because they were the obvious
suspects and they are the counters my retained-shaping work leans on:
`assert_eq!(shape_rebuilds(), N)` and `assert_eq!(lines_reshaped(), N)`.

These are the local analogue of the `assert_eq!(wakes, 0)` trap — a counter
compared to a literal. **They are not timing ceilings.** `shape_rebuilds`
increments in exactly one place, `compositor/shaping.rs:33`, inside
`rebuild_retained`, which runs only when a `ShapeKey` misses. The key's terms
are content, viewport range, wrap width, font, size, theme, folds and gutter
— **no term is derived from a clock.** Verified by reading the increment site
and the key, not by reading the test names.

`view/frame_timer.rs:392,403` — `assert!(diff < Duration::from_micros(1))` —
also **stand**, and are the inverse trap: the name, the `Duration` and the
microsecond tolerance all read as a timing assertion, but `target_duration()`
is arithmetic on the fps constant and never touches the clock.

`iridium-explorer/src/tests.rs` `PATIENCE`-bounded crawl loops **stand**. The
10 s is harness patience, not the claim; what the assertion demands is that
the crawl *completes*. Load can only push those toward red, so a green is
sound evidence. Their **reds** would be uninterpretable — the mirror case.

`frame_timer.rs:480` (`dropped_frames() > 0` after oversleeping the target)
and `:414,429` (`elapsed >= 1ms`, `avg_frame_time >= 100µs`) **stand**: all
are lower bounds, and load only makes them more true.

### Greens that are VOID — six timing ceilings in `view/frame_timer.rs`

Every one asserts an **upper bound on wall-clock elapsed between two adjacent
statements**, so a green says nothing unless the box's load is declared, and
mine never was.

| site | assertion | why it is a ceiling |
| --- | --- | --- |
| `:415` | `elapsed < Duration::from_millis(10)` | explicit |
| `:456` | `(0.5..=2.0).contains(&scaled)` | upper end demands < 20 ms elapsed |
| `:469` | `dt.delta() <= Duration::from_millis(60)` | explicit |
| `:438` | `assert_eq!(timer.current_budget(), FrameBudget::Comfortable)` | ⚠️ demands < 7 ms elapsed |
| `:445` | `assert!(timer.has_budget_for_optional_work())` | ⚠️ demands < 7 ms elapsed |
| `:499` | `assert!(remaining > Duration::ZERO)` | ⚠️ demands < 8.33 ms elapsed |

The last three are the trap in its pure form: **an enum equality, a bare
bool, and a positivity check — three shapes that look like value checks and
are all deadlines.** `frame_budget_comfortable` is an `assert_eq!` against an
enum variant and is a 7 ms ceiling. Nothing in the name, the type or the
comparison discloses it.

These are not only uninterpretable — they are **latent flakes**. At the load
this box was carrying when the audit ran (1-min 19.28, then 12.44, against 10
cores), a scheduler preemption between `begin_frame()` and the assertion
turns `:438`, `:445` and `:499` red for no reason connected to the code.

**The fix already has an in-tree precedent.** `render/cursor.rs` takes the
instant explicitly — `renderer.update(start + Duration::from_millis(25))` —
so its blink tests are load-independent by construction. `FrameTimer` has no
such seam: it reads `Instant::now()` internally at `with_target_fps:131`,
`begin_frame:160`, `end_frame:169` and `elapsed:196`. Giving it the seam
`CursorRenderer` already has removes all six ceilings and makes their verdicts
interpretable at any load. Filed as its own task.

### A third category the ruling does not name: silent oracle weakening

`tests/retained_shaping/hits.rs:75` sleeps **600 ms** to reach "a frame later
in the blink cycle". The blink interval is **1 s** with a 500 ms half
(`render/cursor.rs:53,158,161`), so 600 ms is chosen to land in the opposite
half from the start.

Under load, a sleep that overshoots **1 s** wraps `cycle_position` back into
the *same* half it started in. The assertion — `shape_rebuilds() == 1`, that
blink is not a shaping input — **still passes, and is still true.** The test
does not go red. It quietly stops discriminating, because both frames are now
at the same blink phase.

So this is neither STAND nor VOID as the ruling defines them: the verdict is
correct either way, and what load costs you is the test's *power*, not its
answer. Recording it because it is invisible to any audit that classifies by
what the assertion demands — the demand is met; the setup is what decayed.
Same shape as the oracle hole I found in my own explorer test last stint,
where a deliberately flattened depth still passed.

There is no seam here to fix it with: `compositor/frame.rs:192` calls
`cursor_renderer.update(Instant::now())` internally, so the compositor test
cannot inject a phase the way `cursor.rs`'s own unit tests can.

## The ≥300 s debounce, re-derived here — and it does not hold

The doc above says the 300 s was inherited from Cally's box (>3× a widest
observed ~90 s trough) and that I had not re-derived it. I have now.

`<scratchpad>/loadsample.sh` sampled 1-minute load every 30 s. Threshold is
the core count, **10**, read at run time. From 24 minutes on 2026-08-07:

```
19:56:49   9.04  <   trough opens
19:57:19   7.64  <
   ...           <   16 consecutive samples, every one under 10
20:02:49   4.70  <   floor
20:04:19   6.50  <   trough closes — 450 s wide
20:04:49  18.26      back over, and stays over for ten minutes
```

**The widest lull measured here is 450 s against a 300 s debounce.** A gate
running the inherited rule clears at **20:01:49** — three consecutive passing
samples, 300 s spanned, every one of them true — permits a battery, and three
minutes later the box returns to 18.26 and stays above the threshold for ten
minutes. An eight-gate battery is not close to finished in three minutes, so
it would have run straight through the busy period **with a legitimately
passed precondition behind it.**

⭐ **A debounce must be wider than the widest lull that is not real quiet.**
300 s came from a box whose widest was ~90 s. The transferable part of that
rule is the derivation, never the number. Applying the same 3× to what is
measured here gives roughly **1350 s**.

⚠️ **That is one trough, not a distribution.** 24 minutes of sampling is
enough to falsify 300 s for this box and nowhere near enough to establish
1350 s in its place. The number must not travel out of this file the way the
300 s travelled into it.

### Two consequences for whatever the shared preflight becomes

1. **The debounce is a per-box measured parameter, not a script constant.** A
   box's lull structure is a property of what else runs on it. Same script,
   same rule, locally derived number — otherwise every box inherits whichever
   box happened to be characterised first.
2. **Clearance needs re-checking during a long run, not only before it.**
   Every gate discussed so far, mine included, guards the *start*. The run
   above would have passed a real precondition and then spent minutes inside
   a spike with nothing watching. **A precondition that fires once is a
   precondition on the first second of the run.**

## Two corrections that belong in the repo, not only in Meridian

### `4653fc1`'s message overclaims, and the commit is staying as it is

`4653fc1` — *"find_repo_root must test .git with -e, not -d"* — is **a correct
fix with a message that claims more than the code supports.** Its message says
*"a feature created from inside a worktree lands in the parent repo."* That
cannot happen through this script, on two independent grounds, both established
by execution rather than by reading:

1. **`find_repo_root` is the `--no-git` fallback, not the worktree path.**
   `create-new-feature.sh:167` tries `git rev-parse --show-toplevel` first and
   only falls through at `:171` when it *fails*. In a worktree
   `--show-toplevel` succeeds and returns the worktree root, so the fixed
   function is never reached.
2. **When it is reached, the `.git` test cannot decide the outcome.** It is
   called with `SCRIPT_DIR`, always `<root>/.specify/scripts/bash`, and the
   same condition also tests `-d "$dir/.specify"`. Walking up from there,
   `.specify` matches at the true root before any parent. Run from that start
   directory, `-d` and `-e` both return the worktree root.

The divergence appears only from a start directory with no `.specify` above it
— which is the fixture that found it, and not the argument the script passes.

**The fix stays.** `-e` is correct, costs nothing, and is the right defence if
the `.specify` clause ever changes or the function is reused where it is the
only test. The commit is not being rewritten — see below for why history
surgery in a shared tree is the thing that caused the incident in the first
place. This note is the correction of record.

⭐ The fixture proved the **function** diverges and never proved the **script**
reaches the divergence. Mechanism verified, reachability not.

### The `--amend` incident, and why its control was worthless

At 20:17:47 a `git commit --amend` intended to reword `4653fc1` instead
rewrote `a778eac`, because **`--amend` takes no target argument — it always
acts on HEAD, and HEAD had moved.** Repaired by `git reset --soft a778eac`;
verified here at my own hands: HEAD is exactly `a778eac`, and since a commit
hash covers message, tree, parent, author and committer, that one equality
settles every field. Nothing was pushed.

The discipline in force was *commit with explicit paths*, which was followed
correctly and **bought nothing, because `--amend` has no path argument to be
explicit about.** A guard written for one verb and never mirrored onto its
neighbour.

The control taken at the time captured `HEAD^{tree}` before and after and
asserted equality. It passed, and it was true — **both trees were the
victim's.** ⭐⭐ **That control could not have failed:** `--amend` with nothing
staged never changes the tree, so the assertion was a tautology, equally true
on the clobber path and the clean path.

**The question a control must survive is not "did it pass" but "name the
circumstance in which it fails."** A control with no such circumstance is
decoration. This is the same defect as the six timing ceilings above wearing
value-check clothing, and as my explorer oracle that passed against a
deliberately flattened tree: a green that is unconditional.

The working form: **bind the control to the identity of the thing you are
about to change, not to a property the change preserves anyway.**

### The second window closed, which is the mid-run point as evidence

The trough analysis above argued that clearance needs re-checking *during* a
run. It then happened:

```
20:15:20   9.45  <   window opens
20:15:50   7.88  <
20:16:20   8.19  <
20:16:50   9.75  <
20:17:20   8.22  <
20:17:50   6.76  <   window closes — 150 s wide, 6 samples
20:18:20  20.56      over again
20:21:20  30.23
20:22:51  69.85
```

A 150 s window, then the box back to three times the threshold within a
minute and to **seven times** it within five. **Had the battery started when
load first dipped under, it would have been running through all of that.** It
was not started, because the widest lull measured here is 450 s and a 150 s
window does not clear it.

⚠️ **Correction — the first version of this section said "~30 s, 2 samples",
and that figure was wrong.** It was measured at 20:15:50, while the window was
still open, and the trough ran another two minutes afterwards. The script
labelled that reading `[OPEN]`; **I dropped the qualifier when I wrote the
number down.** The conclusion did not depend on it — 150 s clears a 450 s lull
no better than 30 s does — but the figure had already been sent to someone
assembling a distribution from it.

⭐ **A measurement taken while the thing is still happening is a lower bound,
not a value.** Committed here in the document that catalogues the class, one
section below a note on controls that cannot fail. The instrument reported its
own incompleteness and the reporting discarded it.

Two troughs so far — **450 s and 150 s**. **That is not a distribution and no
span should be adopted from it.** Sampling continues.

## Git cannot attribute a commit in this checkout

Measured across the whole history:

```
$ git log --format='%an|%ae|%cn|%ce' | sort -u
tomWhiting|tom.whiting@reachsocialsupports.com.au|tomWhiting|tom.whiting@reachsocialsupports.com.au
```

**One distinct value.** Author and committer, name and email, every commit
ever made here. Several seats commit into this working tree and **not one
git identity field distinguishes them.** Any guard, hook or audit keyed on
who authored a commit cannot fire, which is why a proposed hook refusing an
amend whose HEAD author differs from the amender was killed before it was
built — it would have closed the row while being incapable of firing.

### There is a discriminator, and it is accidental

```
cc69da8  trailers: [Co-Authored-By: Claude Opus 5 (1M context) ...]
a778eac  trailers: [Co-Authored-By: Claude Opus 5 (1M context) ...]
4653fc1  trailers: []                                    ← another seat
```

⚠️ **This is a proxy that agrees with its target only on the examined set.**
The trailer names a **model**, not a **seat**. It separates the commits above
purely because the other seat's carry none. **The circumstance where it
fails: another seat runs the same model through the same harness and emits a
byte-identical trailer.** The discriminator disappears and nothing announces
that it has.

So the *channel* is right and the *content* is wrong. A trailer lives in the
commit object, survives a push and a clone, and shows up in `git log`.

### The instrument that worked tonight does not survive the week

The `--amend` clobber above was verified by reflog. **The reflog is local,
unpushed, and pruned.** It answered the question at the time and would answer
nobody from another clone, or from this one after a `gc`. Anything durable has
to live in the commit object or in a tracked file.

### Adopted here

`Seat:` as a trailer, starting with the commit that adds this section.

**Stated plainly: unilateral adoption gives partial coverage.** My commits
become attributable and no one else's do — which is precisely the accidental
situation described above, only deliberate instead of lucky. It becomes an
instrument when the other seats carry one, and not before.

⭐ **This closes the loop on the question that opened the whole exchange.**
*"Whose run was this?"* was answered with log mtimes and a commit window
because **git could not tell either party.** A seat trailer answers it in one
command, at the time, with no forensics.

## The trough record, and a refinement the data forced

Four closed lulls measured on this box, 2026-08-07, threshold = 10 cores,
30 s sampling. **All four CLOSED — no open reading is quoted here.**

| opened | width | samples |
| --- | --- | --- |
| 19:56:49 | **450 s** | 16 |
| 20:15:20 | **150 s** | 6 |
| 20:33:21 | **601 s** | 21 |
| 20:53:52 | **300 s** | 11 |

Widest is **601 s**, and it closed like the rest. Four points is still not a
distribution, and no span is adopted from it.

### Scale the debounce to the duration of the run it gates

The 20:53 window is the useful one, because work was done inside it.

A fifteen-minute battery was **not** started on a 240 s-old window when the
widest lull that later closed was 601 s. Instead the runs were escalated by
cost: a 15 s `cargo check`, then a 0.00 s test run, then clippy. All four
finished inside the window. **Load hit 31.80 within a minute of it closing.**

⭐ **A long run needs confidence the quiet will persist; a short run mostly
needs the quiet to exist.** The risk being managed is collision, and a run
that finishes in fifteen seconds can barely collide with anything. Treating
both under one debounce either blocks cheap work needlessly or admits
expensive work recklessly — and the single-number rule does the second.

This is the third time in one evening that the mid-run argument has arrived as
evidence rather than as argument: a window that would have passed any
entry-only precondition, followed by a spike the precondition could not see.

## A multi-window gate never clears on this box

Measured over 142 samples, 71 minutes, 30 s interval, 10 cores, threshold =
core count:

| window | samples under threshold |
| --- | --- |
| 1-minute | 54 / 142 — **38 %** |
| 5-minute | 6 / 142 — **4 %** |
| **15-minute** | **0 / 142 — 0 %** |
| 1m AND 5m | 6 / 142 — 4 % |
| **1m AND 5m AND 15m** | **0 / 142** |

**The 15-minute average never came under 10.** A long run gated on it is
refused permanently — not conservatively, *permanently* — and it fails closed
silently, which is the shape that reads as a working gate. Even 1m AND 5m
leaves about three usable minutes in seventy-one.

### The threshold cannot be `cores` for every window

⭐ **The longer the averaging window, the less `load == cores` means "busy
now."** A 1-minute average at cores says the box is saturated this instant. A
15-minute average at cores says it was busy *at some point in the last quarter
hour*, which on a shared box with several active seats is very nearly always
true. **One number applied across different windows measures different
things** — so it is not conservatism, it is a category error that happens to
point in the refusing direction.

Each window past the first needs a threshold **derived for that window**, not
inherited from the core count.

⚠️ **Caveat:** these 71 minutes include another seat's legitimate battery, so
the 15-minute figure was genuinely elevated. One box, one evening, one heavy
neighbour. **That is the condition a preflight has to decide well in** — a
gate that only works on a quiet box is not doing anything a quiet box needed.

### The same instrument, two readings, opposite correct answers

| time | reading | run | correct? |
| --- | --- | --- | --- |
| 20:55 | `1m=5.55 5m=13.28 15m=17.12` | 15 s `cargo check` — moved load 6.28 → 6.55 | **refusing it would be wrong** |
| 20:58 | `1m=9.45 5m=11.18 15m=15.47` | 15-minute battery | **refusing it was right** — load hit 31.80 minutes later |

Near-identical readings. The only thing separating them is **how long the run
was going to take**. Duration is not a refinement of the gate; without it the
gate cannot distinguish these two cases at all.
