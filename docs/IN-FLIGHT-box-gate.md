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
