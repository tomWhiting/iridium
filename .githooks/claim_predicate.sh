# shellcheck shell=sh
# THE CLAIM PREDICATE — #158. SOURCED, NEVER EXECUTED.
#
# ⚖️ RULED at ablative/docs `f3016c7`, amended `15b0fd5` and `63c6339` (Waffles
# the Terrible, 2026-08-08): the shared-tree claim is carried by
# `refs/guards/shared-tree` in the repository's COMMON ref store — never by a
# tracked file. A tracked file forks with every branch, and each fork asserts
# authority over the whole repository.
#
# ⭐ ONE IMPLEMENTATION, SOURCED BY EVERY CONSUMER. This file exists because
# `reference-transaction` and `check_guard_active.sh` shared a defect BY SHARING
# A PREDICATE THEY EACH HAND-KEPT (Finding 22). Two copies of a rule drift; the
# fix has to be structural or it is the same bug waiting. Ruling point 1.
#
# ⭐⭐ POINT 2 IS SATISFIED BY DELETING THE DEPENDENCE, NOT BY SWAPPING HELPERS.
# The obvious reading of "`--show-toplevel` may not appear in any instrument
# whose protected object lives in the common dir" is to reach for
# `--git-common-dir` instead. MEASURED: `refs/guards/*` is not a per-worktree
# ref, so `rev-parse` resolves it identically from every worktree of a repo with
# NO PATH INVOLVED AT ALL. So this predicate takes no root, derives no root, and
# cannot be wrong about one. A dependence removed cannot be re-broken by a later
# edit; a dependence re-derived can.
#
# ⛔ WHAT THIS PREDICATE CANNOT SEE, STATED HERE BECAUSE IT IS LOAD-BEARING.
# MEASURED 2026-08-08: deleting the claim is SILENT AND UNATTRIBUTABLE.
#   · default `core.logAllRefUpdates=true` writes NO reflog for `refs/guards/*`;
#   · `always` writes one, and `git update-ref -d` DESTROYS IT with the ref;
#   · afterwards there is no ref, no `logs/refs/guards/*`, and no mention in
#     HEAD's reflog. The blob survives only to someone who already knows its hash.
# The superseded tracked-file design made a permanent disarm a COMMIT — visible,
# reviewable, attributable. THE REF HAS NO SUCH WITNESS AT THE MOMENT OF THE ACT.
#
# ⭐ WHERE THE COMPENSATING WITNESS LIVES — never state a blind spot without it,
# or the reader is sent to despair instead of to the record (Waffles, `b63bf0d`).
# The property is RELOCATED, not lost: the legitimate disarm of a shared
# repository is AN EDIT TO THE MUST-BE-CLAIMED LIST, which is a commit in the
# estate ledger — visible, reviewable, attributable, exactly as before. A ref
# deleted WITHOUT that edit is caught by `check_must_be_claimed.sh` — ⛔ WHEN
# SOMETHING RUNS IT, WHICH TODAY NOTHING DOES. See the measured block below
# before quoting this paragraph; it is a design, not a live control.
#
# ⛔ WHAT IS GENUINELY LOST, STATED RATHER THAN SOFTENED: detection is now
# PERIODIC, NOT ACT-TIME. The tracked file refused at the moment of the act; the
# list catches it at the next battery run. That window is real latency, and the
# list's coverage and the battery's cadence are therefore GUARD PROPERTIES now,
# not test hygiene. `core.logAllRefUpdates=always` does not close it — it is
# local, non-travelling, and never load-bearing.
#
# ⛔⛔ AND "THE NEXT BATTERY RUN" DESCRIBES A RUN THAT DOES NOT CURRENTLY HAPPEN.
# MEASURED 2026-08-08, estate-wide: NOTHING INVOKES ANY OF THESE INSTRUMENTS.
# Every reference to `check_guard_active.sh`, `check_must_be_claimed.sh`,
# `sweep_guard_active.sh` or `census_claim_predicate.sh` outside this directory
# is PROSE IN A DOCUMENT, and NO BATTERY CONFIG ANYWHERE CARRIES A GUARD LEG.
# ⭐ SO THE LATENCY IS NOT PERIODIC — IT IS UNBOUNDED, and the two paragraphs
# above overstate the protection until a battery calls these. Stated HERE, in the
# text a reader comes to for reassurance, because a compensating control nobody
# invokes is indistinguishable from one that was never built. WIRING IS OPEN.
#
# ⚠️ CORRECTED SAME DAY — the sentence above previously read "the only
# `.gates.json` files in the estate are five examples". THAT WAS FALSE, and the
# error was in the instrument, not the arithmetic: the scan matched
# `*.gates.json`, which is the EXAMPLE naming convention (`<repo>.gates.json`
# under `examples/`). A live config is `gates.json` at the repo root — exactly
# what the README's worker flow instructs — so the scan COULD NOT have matched
# one. RE-MEASURED: SEVEN live battery configs (aion, frame, liminal, haematite,
# beamr, meridian, meridian-tools-seat), FOUR with committed report.json from
# 2026-07-28. ⭐⭐ A FALSE DENOMINATOR DOES NOT ONLY MIS-STATE COVERAGE — IT
# MISDIRECTS THE REMEDY: "there is no battery to wire into" points at building a
# cadence, and the truth ("seven exist and four have run") points at adding a leg.
# The conclusion survived its own broken evidence, which is the more dangerous
# outcome, because nothing about the verdict looked wrong.
#
# ⭐ AND THE CADENCE, ONCE WIRED, IS EVENT-DRIVEN, NOT PERIODIC. The battery runs
# at landing, by hand (RUNBOOK: "one battery at the release tree"). So the latency
# after wiring is "until this repo's next landing" — bounded for an active repo
# and STILL UNBOUNDED FOR A DORMANT ONE. ⛔ That is the wrong way round: a shared
# tree nobody has landed in for a month is precisely where a silent unclaim
# survives longest. A landing-time leg is necessary and is not sufficient.

# The ruled ref. Named once, in one place, so a typo is a single-site failure
# rather than a silent disagreement between two instruments.
CLAIM_REF='refs/guards/shared-tree'

# claim_is_set [repo]
#
# TRUE  (0) — this repository carries the claim; the guard applies here.
# FALSE (1) — it does not. ⛔ That is the LEGITIMATE unclaimed state (ruling
#             point 3) and NOT an error: a repo nobody shares should skip the
#             guard. Whether a given repo OUGHT to be claimed is information
#             that exists in no repository — see the block above.
#
# Takes an optional repo path for out-of-tree callers (the detector and the
# sweep run from outside the repo they measure). Passing nothing measures the
# repository the caller is standing in.
claim_is_set() {
    if [ "${1:-}" = "" ]; then
        git rev-parse --verify --quiet "$CLAIM_REF" >/dev/null 2>&1
    else
        git -C "$1" rev-parse --verify --quiet "$CLAIM_REF" >/dev/null 2>&1
    fi
}

# claim_record [repo]
#
# Emits the claim blob's contents — seat, date, scope — on stdout. Empty and
# non-zero when unclaimed. ⭐ The claim is an ASSERTING SURFACE, so it carries
# its own record: a reader who finds a repo guarded can ask WHO claimed it and
# WHY without leaving the repository. That property is real and survives the
# reflog finding above, because it lives in the blob rather than in the ref's
# history — what does NOT survive is any record of the claim being REMOVED.
claim_record() {
    if [ "${1:-}" = "" ]; then
        git cat-file -p "$CLAIM_REF" 2>/dev/null
    else
        git -C "$1" cat-file -p "$CLAIM_REF" 2>/dev/null
    fi
}
