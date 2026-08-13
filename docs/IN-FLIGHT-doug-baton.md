# Baton — seat Doug, 13 Aug 2026

Written at the context ceiling. Everything below is verified, not recalled.

## Where the repository is

| ref | SHA | state |
| --- | --- | --- |
| `main` | `31113902` | pushed, local == remote, **SHA-verified** |
| `guard-orphan-probe2` | `05e9833f` | pushed; #89's guard probe, its own message says safe to orphan |
| `backup/claude-skills` | `915b8b71` | pushed; see below |

Working tree clean apart from `.claude/skills/` (untracked, deliberately).
No stashes.

### ⚠️ `backup/claude-skills` — what it is and why

Tom asked twice, ahead of a restart he was not sure his laptop would return
from, to get **everything** pushed. `.claude/skills/norn/` (25 files, Norn's
skill definitions) was untracked and therefore existed only on that disk. The
standing rule keeps it out of Iridium, so it went to a side branch, **not**
`main`, and nothing merges it. Delete with
`git push origin --delete backup/claude-skills`.

📌 **The method matters and is reusable.** The commit was built through a
temporary index — `GIT_INDEX_FILE=<path> git read-tree --empty`, then
`git add <paths>`, `git write-tree`, `git commit-tree`, `git branch <name>
<sha>`. HEAD never moved, the real index was never touched, the working tree
was never touched. Doing it by switching branches would have **deleted those 25
files off disk** on the way back to `main`: git removes files tracked on the
branch you leave that do not exist on the branch you join. Verified all 26 files
still present afterwards.

## What owes a run

⚠️ **The ten gates have NOT run on `31113902`.** That commit (the terminal
search split) has crate tests, clippy (exit 0, zero warnings) and fmt green.
The battery is 15–20 minutes and Tom wanted everything pushed before a reboot.
The change is test files in one crate so it cannot reach the wasm or
kernel-feature gates, but that is reasoning, not a measurement. **Run
`./scripts/ci.sh` first thing.**

## Closed this session

* **#117** — `d22baa93`. The desktop right-click menu was the last panel key
  table the task could reach. All six reachable panels now on `[keys]`. The
  menubar is panel 7 and stays untouched until #108 clears.
* **#95** — `e40dd9da`. One comment carried numbers true only of one grid;
  fifteen other `MEASURED` sites were read, judged mechanism facts, and kept.
  The verdicts are listed in the commit message so nobody re-litigates them.
* **#92, four slices** — `f6a17ffe`, `37428086`, `ebdbd68a`, `31113902`.

## 📌 Laws this session earned

1. **A restore that carries an old mtime is invisible to the build system.**
   `mv backup original` restored the content *and* the old timestamp, so cargo
   judged the crate fresh and re-ran the **mutated** binary. Rewrite in place
   when reverting, or `touch` afterwards. The dangerous direction is the other
   one: a mutation written with a stale mtime measures as *not caught* — a
   false clean.

2. **An orphaned `.rs` in a module directory compiles clean and takes its tests
   with it.** Measured: commenting out one `mod` line left the suite **green**
   at 1,368 instead of 1,376 — eight tests gone, nothing red, and neither the
   test gate nor clippy nor fmt said a word. So a green suite is not evidence a
   split preserved anything; only a count either side is. Now in
   `docs/CODING_STANDARDS.md`. Swept the whole tree for pre-existing orphans
   across both module forms, 389 declared children: none.

3. **`grep … | head; echo "clean"` is a success-only channel.** The `echo` runs
   whatever the grep found. A real clippy warning printed and the line under it
   still said clean. Redirect to a file, then read the file.

## Open, and waiting on Tom

**#119 — `docs/design/CONFIG-WRITER-MAP.md`, ruling D-2.** Sent 13 Aug, no
answer yet.

Verified ground: `apps/iridium` never calls `create_if_absent`, so a
terminal-only user has **no `config.toml` at all**. Every generated `[keys]`
line is commented out, so one file listing both faces' bindings makes neither
face complain — the obvious objection is not real. What remains is a
dependency-direction problem plus thirteen face-bound commands.

⚠️ **D-1 is not shippable alone**: a terminal writer handing over only its own
layers gives a Mac user who ran `iridium` first a file with no ⌘ chords, which
regresses today. Recommended **B** — the four face-specific panel keymaps move
below both faces; each face's own top layer stays handed in.

## #92 — next slices, planned

Still over the 1,000 hard limit, worst first:

```
3185  crates/iridium-bindings/src/wasm.rs        production, needs a design pass
2876  crates/iridium-editor/src/editor/core.rs   production, needs a design pass
1399  crates/iridium-editor/src/render/text.rs
1268  crates/iridium-editor/src/input/mouse.rs
1061  apps/iridium/src/app/tests.rs               ⚠️ no section rules — plan first
1015  crates/iridium-bindings/src/editor.rs
1013  crates/iridium-editor/src/render/highlight.rs
```

⚠️ **Do not hand-split `render/highlight.rs`** — the plan file's syntax-tree
refactor (§4.1) drops it to ~640 as a by-product.

**The method that worked four times**, in order:

1. Census the file's own `// ===== … =====` rules and **every** top-level item.
   Use a regex that catches `pub(super) fn`, not just `^fn ` — the first pass
   missed `doc_in` and `press` that way.
2. Find which shared helpers sibling modules import; those must land in
   `mod.rs`, because `pub(super)` from a directory's `mod.rs` still means
   visible in the parent.
3. `cargo test -p <crate> --lib -- --list` **before**.
4. Slice by line range in Python, asserting each boundary against the text that
   must be on it.
5. Children get `use super::*;` — nothing else. A module-level
   `#![allow(...)]` in `mod.rs` does reach the child files; confirmed on clippy
   `--all-targets`.
6. Census **after**: total, subtree, and a leaf-name diff.
7. `cargo fmt --all`, clippy **redirected to a file and read**, commit, gates.

## Standing constraints that must not be lost

* Ports **3000, 3030, 8000, 8080** are banned. **Vite ban** — run-once builds
  only.
* Never run `git stash`, `checkout`, `restore`, `reset`, `clean`, `worktree`,
  `add -A`, or `rebase` in this checkout. Stage explicit paths.
  `git show HEAD:path > path` is the only sanctioned restore.
* Never end a `ci.sh` invocation with `; echo exit=$?` — the harness then
  reports `echo`'s status.
* Read the gate verdict from the runner's own output:
  `grep -E "^(>>>|!!!|✅|⛔)" <file>`.
* zsh: `$pipestatus`, not `$PIPESTATUS`. Quote globs. Name shell vars `rc`,
  never `status`.
* Anything for Tom leaves through the Meridian `send` tool or it did not happen.
  Tom `dm:c9255b2a-5731-4d17-8124-e3bfa2224186`.
* Commits carry a `Seat: Doug` trailer.
* Never kill pids 99844 or 31161. `pgrep -x`, never `-f`.
* `.claude/skills/` stays untracked and off `main`.
* Decide rather than asking Tom to pick; record the reasoning and the revert
  cost. Code first in a tick, messages last.
